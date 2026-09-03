//! The Vilan language server: a thin tower-lsp front-end over `vilan-core`.
//! Analyzes each open document on change and answers diagnostics, hover,
//! go-to-definition, find-references, and rename — across files into `std`.

#[cfg(test)]
mod book_sync;
mod document;
mod keystroke;
mod line_index;
mod manifest_completion;
mod publish;
mod references;
mod session_trace;
mod uri;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use tower_lsp::jsonrpc::{Error as JsonRpcError, ErrorCode};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server, jsonrpc::Result};
use vilan_core::Span;
use vilan_core::analyzer::SourceId;

use crate::document::{Document, Symbol, SymbolKind as VilanSymbolKind, hash_text};
use crate::line_index::LineIndex;
use crate::publish::PublishState;
use vilan_ide::{Completion, CompletionFunctionCall, CompletionKind as VilanCompletionKind};

/// How long to wait after the last edit before re-analyzing, so a burst of
/// keystrokes collapses to a single analysis instead of one per character.
const DEBOUNCE_MS: u64 = 150;

/// The client's feature settings (VS Code `contributes.configuration`), received
/// as `initializationOptions` at startup and refreshed live by
/// `workspace/didChangeConfiguration`. Defaults preserve today's behavior: every
/// provider on, full function-call completion. (`organizeImports.onSave` is a
/// client-only concern — `editor.codeActionsOnSave` — so the server never reads
/// it.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Config {
    inlay_hints_enabled: bool,
    semantic_tokens_enabled: bool,
    completion_function_call: CompletionFunctionCall,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            inlay_hints_enabled: true,
            semantic_tokens_enabled: true,
            completion_function_call: CompletionFunctionCall::Full,
        }
    }
}

impl Config {
    /// Parses the settings object the client sends. Accepts either the bare
    /// `vilan` config (as `initializationOptions`) or a `{ "vilan": { … } }`
    /// wrapper (as `didChangeConfiguration`'s `settings`). Every field falls back
    /// to its default when absent or the wrong type, so a partial or malformed
    /// payload never silently flips a provider off.
    fn from_settings(settings: &serde_json::Value) -> Self {
        let root = settings.get("vilan").unwrap_or(settings);
        let mut config = Config::default();
        if let Some(enabled) = root
            .pointer("/inlayHints/enabled")
            .and_then(|v| v.as_bool())
        {
            config.inlay_hints_enabled = enabled;
        }
        if let Some(enabled) = root
            .pointer("/semanticTokens/enabled")
            .and_then(|v| v.as_bool())
        {
            config.semantic_tokens_enabled = enabled;
        }
        if let Some(mode) = root
            .pointer("/completion/functionCall")
            .and_then(|v| v.as_str())
        {
            config.completion_function_call = match mode {
                "none" => CompletionFunctionCall::None,
                "parensOnly" => CompletionFunctionCall::ParensOnly,
                // `full` and any unrecognized value keep the default.
                _ => CompletionFunctionCall::Full,
            };
        }
        config
    }
}

/// Convert a manifest completion candidate to an LSP `CompletionItem` (F5 S5).
/// The item carries an explicit `TextEdit` rather than a bare insertion: a
/// manifest value owns its quotes, so what gets replaced is decided by the
/// schema side (`manifest_completion::completions`), never by the client's idea
/// of where a word starts.
fn to_manifest_item(
    completion: manifest_completion::ManifestCompletion,
    line_index: &LineIndex,
) -> CompletionItem {
    let range = line_index.range(&Span::from(completion.replace));
    CompletionItem {
        label: completion.label,
        kind: Some(if completion.is_key {
            CompletionItemKind::PROPERTY
        } else {
            CompletionItemKind::VALUE
        }),
        documentation: completion.documentation.map(Documentation::String),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: completion.insert,
        })),
        ..Default::default()
    }
}

/// Convert a Vilan completion candidate to an LSP `CompletionItem`, applying the
/// `vilan.completion.functionCall` setting and the client's snippet capability
/// to shape a function/method insertion (WO-3). The popup always carries the
/// candidate's signature/type `detail` and `///` documentation.
fn to_completion_item(
    completion: Completion,
    mode: CompletionFunctionCall,
    snippet_support: bool,
    line_index: &LineIndex,
) -> CompletionItem {
    let kind = match completion.kind {
        // The LSP kind set has no macro entry; functions render closest.
        VilanCompletionKind::Macro => CompletionItemKind::FUNCTION,
        VilanCompletionKind::Function => CompletionItemKind::FUNCTION,
        VilanCompletionKind::Method => CompletionItemKind::METHOD,
        VilanCompletionKind::Field => CompletionItemKind::FIELD,
        VilanCompletionKind::Struct => CompletionItemKind::STRUCT,
        VilanCompletionKind::Enum => CompletionItemKind::ENUM,
        VilanCompletionKind::EnumVariant => CompletionItemKind::ENUM_MEMBER,
        VilanCompletionKind::Trait => CompletionItemKind::INTERFACE,
        VilanCompletionKind::Variable => CompletionItemKind::VARIABLE,
        VilanCompletionKind::Module => CompletionItemKind::MODULE,
        VilanCompletionKind::Keyword => CompletionItemKind::KEYWORD,
        VilanCompletionKind::Snippet => CompletionItemKind::SNIPPET,
    };
    let mut item = CompletionItem {
        label: completion.label.clone(),
        kind: Some(kind),
        detail: completion.detail,
        documentation: completion.documentation.map(Documentation::String),
        ..Default::default()
    };
    // A call-shaped insertion applies only to a callable in a call position
    // (`call_parameters` is `Some`) and only when the setting asks for it. `none`
    // keeps today's bare-name insertion. With parameters, a signature-help popup
    // is triggered so the user sees what to fill.
    if let Some(parameters) = completion.call_parameters {
        let call = call_insertion(&completion.label, &parameters, mode, snippet_support);
        if let Some((insert_text, format)) = call {
            item.insert_text = Some(insert_text);
            item.insert_text_format = Some(format);
            if !parameters.is_empty() {
                item.command = Some(Command {
                    title: "Trigger Parameter Hints".to_string(),
                    command: "editor.action.triggerParameterHints".to_string(),
                    arguments: None,
                });
            }
        }
    }
    // A construct snippet (E14) inserts its tab-stopped body when the client can
    // expand snippets, else the bare keyword (a `${1:…}` body would land as
    // literal text). Its `sort_text` starts with `~` so it sorts after every
    // entity and keyword (whose labels start with alphanumerics, all below `~`) —
    // the snippet is offered alongside the names in scope without burying them.
    if let Some(snippet) = completion.snippet {
        let (insert_text, format) = if snippet_support {
            (snippet.body, InsertTextFormat::SNIPPET)
        } else {
            (snippet.fallback.clone(), InsertTextFormat::PLAIN_TEXT)
        };
        item.insert_text = Some(insert_text);
        item.insert_text_format = Some(format);
        item.sort_text = Some(format!("~{}", snippet.fallback));
    }
    // An auto-import candidate (E54c): LABEL it with the module it comes
    // from (overriding any signature/type `detail` — the point here is
    // making the import visible, not the candidate's shape) and carry the
    // edit that adds it, so accepting the completion adds the import in the
    // same keystroke. Ranked below every in-scope candidate (which sorts by
    // its bare label, `sort_text: None` falling back to it per the LSP spec)
    // and above a construct snippet (`~`-prefixed, above): `|` (0x7C) sits
    // between every label's leading alphanumeric and `~` (0x7E) in ASCII.
    //
    // WITHIN that `|` band, `origin_tier` (E59) buckets before the label:
    // a single digit right after `|` sorts every candidate of a lower tier
    // (the user's own `pkg`) strictly before every candidate of a higher one
    // (`std`'s always-loaded surface), regardless of what the labels
    // themselves are — a plain per-label sort could never do this, since
    // `std`'s capitalized prelude names (`Add`, `BitAnd`, …) sort ahead of
    // an ordinary lowercase identifier in bare string order. The same tier
    // also drives which candidates survive `AUTO_IMPORT_COMPLETION_CAP`'s
    // truncation (`Document::auto_import_completions`) — one field, read in
    // both places, so the server's own candidate order and the client's
    // displayed order can't drift apart.
    if let Some(auto_import) = completion.needs_import {
        item.detail = Some(auto_import.module_path.join("::"));
        item.additional_text_edits = Some(vec![TextEdit {
            range: line_index.range(&auto_import.edit_span),
            new_text: auto_import.edit_replacement,
        }]);
        item.sort_text = Some(format!("|{}{}", auto_import.origin_tier, item.label));
    }
    item
}

/// The insert text (and its format) for a call-shaped completion, or `None` when
/// the setting is `none` — leaving the bare label. The rule itself is
/// `vilan_ide::call_insertion`, shared with the playground (K9); this maps its
/// snippet flag to the wire's `InsertTextFormat`.
fn call_insertion(
    label: &str,
    parameters: &[String],
    mode: CompletionFunctionCall,
    snippet_support: bool,
) -> Option<(String, InsertTextFormat)> {
    let insertion = vilan_ide::call_insertion(label, parameters, mode, snippet_support)?;
    let format = if insertion.is_snippet {
        InsertTextFormat::SNIPPET
    } else {
        InsertTextFormat::PLAIN_TEXT
    };
    Some((insertion.text, format))
}

/// Delta-encode classified spans (E2) into the LSP semantic-token wire form:
/// five integers per token — line delta, character delta, length, type,
/// modifiers — each relative to the token before it.
///
/// `index` must be the ANALYZED snapshot's line index: the spans are program
/// spans, so that is the text they index (S1). Positions AND `length` are in
/// UTF-16 code units, which is the unit the protocol specifies and the one the
/// line index already produces; the byte width this used to send overshot on
/// every line carrying an accent or an emoji, dragging a token's highlight over
/// its neighbours.
fn encode_semantic_tokens(
    tokens: &[(Span, crate::document::TokenKind, u32)],
    index: &LineIndex,
) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(tokens.len());
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;
    for (span, kind, modifiers) in tokens {
        let range = index.range(span);
        // A token spanning lines has no wire form at all (one line delta, one
        // width). The classifier only ever produces name-sized spans, so this
        // is unreachable — dropped rather than encoded at a bogus width, and
        // never a panic. Saturating deltas below are the same totality: the
        // tokens are sorted (pinned in `document.rs`), and a language server
        // must not abort if that ever stops being true.
        if range.end.line != range.start.line {
            continue;
        }
        let line = range.start.line;
        let start = range.start.character;
        let delta_line = line.saturating_sub(previous_line);
        let delta_start = if delta_line == 0 {
            start.saturating_sub(previous_start)
        } else {
            start
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: range.end.character.saturating_sub(start),
            token_type: *kind as u32,
            token_modifiers_bitset: *modifiers,
        });
        previous_line = line;
        previous_start = start;
    }
    data
}

/// The delta between two encoded token streams, as ONE minimal edit: trim
/// the common prefix and suffix (in whole tokens), replace the middle. The
/// wire counts RAW INTEGERS, five per token (`SemanticTokensEdit.start` /
/// `delete_count` index the flat data array the tokens serialize into), so
/// the token counts convert at the edge. Identical streams are zero edits —
/// the cheapest possible answer to an unchanged refresh (backlog B39b).
fn token_delta(previous: &[SemanticToken], current: &[SemanticToken]) -> Vec<SemanticTokensEdit> {
    let prefix = previous
        .iter()
        .zip(current.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = previous[prefix..]
        .iter()
        .rev()
        .zip(current[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    if prefix == previous.len() && previous.len() == current.len() {
        return Vec::new();
    }
    vec![SemanticTokensEdit {
        start: (prefix * 5) as u32,
        delete_count: ((previous.len() - prefix - suffix) * 5) as u32,
        data: Some(current[prefix..current.len() - suffix].to_vec()),
    }]
}

/// A fresh `result_id` for a semantic-token response — process-unique is all
/// the protocol asks (a client echoes it verbatim on the next delta request).
fn fresh_result_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed).to_string()
}

/// Convert a Vilan outline node to an LSP `DocumentSymbol`.
#[allow(deprecated)]
fn to_lsp_symbol(symbol: Symbol, line_index: &LineIndex) -> DocumentSymbol {
    let kind = match symbol.kind {
        VilanSymbolKind::Function => SymbolKind::FUNCTION,
        VilanSymbolKind::Struct => SymbolKind::STRUCT,
        VilanSymbolKind::Field => SymbolKind::FIELD,
        VilanSymbolKind::Enum => SymbolKind::ENUM,
        VilanSymbolKind::Trait => SymbolKind::INTERFACE,
    };
    let children = symbol
        .children
        .into_iter()
        .map(|child| to_lsp_symbol(child, line_index))
        .collect::<Vec<_>>();
    DocumentSymbol {
        name: symbol.name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: line_index.range(&symbol.full),
        selection_range: line_index.range(&symbol.selection),
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

/// An open `vilan.toml`: its text and a line index, which is everything
/// manifest completion needs (no analysis, no diagnostics of its own — the
/// manifest's diagnostics come from the packages that read it, see
/// `document::ManifestProblem`).
struct ManifestDocument {
    text: String,
    line_index: LineIndex,
}

impl ManifestDocument {
    fn new(text: String) -> ManifestDocument {
        ManifestDocument {
            line_index: LineIndex::new(&text),
            text,
        }
    }
}

/// The refusal a program-space *mutating* request answers while the live buffer
/// has advanced past the analyzed snapshot (S3): its edits would be computed
/// against one text and applied to another. Two spellings, chosen by how the
/// client surfaces them:
///
/// - [`still_analyzing`] — `-32803`, the protocol's `RequestFailed`, spelled
///   through `ServerError` because that one really is absent from tower-lsp
///   0.20's `ErrorCode`. For requests the user invoked EXPLICITLY (rename):
///   `vscode-languageclient` rethrows it without a toast (`rename.js` passes
///   `showNotification: false`) and the rename widget shows this message
///   inline — the user learns why nothing happened.
/// - [`content_modified`] — the named `ContentModified` variant, the code LSP
///   defines for "the content changed under this request". For requests that
///   fire AUTOMATICALLY (code actions: menu population, `organizeImports.onSave`,
///   `codeActionsOnSave`): the client swallows it silently and uses the default
///   empty answer (`handleFailedRequest`), so a save mid-typing is a clean
///   no-op. Any other code on this path raises an error toast — with
///   `showNotification` defaulted true, "Request textDocument/codeAction
///   failed." would pop on every save inside the debounce window.
fn still_analyzing() -> JsonRpcError {
    JsonRpcError {
        code: ErrorCode::ServerError(-32803),
        message: "still analyzing this file; retry in a moment".into(),
        data: None,
    }
}

/// See [`still_analyzing`]: the silent spelling, for refusals of automatic
/// requests. The message rides along for wire logs; clients don't show it.
fn content_modified() -> JsonRpcError {
    JsonRpcError {
        code: ErrorCode::ContentModified,
        message: "still analyzing this file; retry in a moment".into(),
        data: None,
    }
}

/// A rename that cannot produce a COMPLETE edit set refuses, carrying the
/// reason (kolt.local 002). The same `-32803` `RequestFailed` spelling rename's
/// other refusals use, so the client surfaces the message inline on the rename
/// widget rather than as a toast — the user is standing at the symbol, and the
/// reason is about that symbol.
fn rename_refused(refusal: &crate::document::RenameRefusal) -> JsonRpcError {
    JsonRpcError {
        code: ErrorCode::ServerError(-32803),
        message: refusal.message().into(),
        data: None,
    }
}

/// The refusal a PANICKED edit-producing handler answers with (B40). Rename
/// and formatting return edits; their empty answer reads as "nothing to do",
/// which a failure is not — so they refuse, in the inline no-toast spelling
/// `still_analyzing` documents.
fn handler_panicked() -> JsonRpcError {
    JsonRpcError {
        code: ErrorCode::ServerError(-32803),
        message: "the request failed inside the language server; this is a vilan-lsp bug".into(),
        data: None,
    }
}

/// The human part of a panic payload (`panic!` carries a `&str` or `String`;
/// anything else renders opaquely).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic payload>")
}

/// Whether `uri` names a `vilan.toml`. The extension registers manifests with
/// the server by PATH (any language id), so this is the routing question every
/// document notification asks first.
fn is_manifest(uri: &Url) -> bool {
    uri.path()
        .rsplit('/')
        .next()
        .is_some_and(|name| name == "vilan.toml")
}

struct Backend {
    client: Client,
    documents: Arc<DashMap<Url, Document>>,
    /// What `semanticTokens/full` (or a delta response) last SENT per
    /// document, keyed by its `result_id` — the delta path's baseline
    /// (backlog B39b). Evicted on close; a stale or unknown id simply
    /// answers full again.
    semantic_token_cache: Arc<DashMap<Url, (String, Vec<SemanticToken>)>>,
    /// Open `vilan.toml` buffers (F5 S5). A manifest is TOML, not vilan: it is
    /// kept HERE rather than in `documents` so nothing ever hands it to
    /// `Document::analyze`, which would publish a wall of lexer errors on a
    /// perfectly good manifest. Completion is all it feeds.
    manifests: Arc<DashMap<Url, ManifestDocument>>,
    /// The latest edit generation per document, so a debounced analysis can tell
    /// whether a newer edit (or a close) has superseded it before it runs.
    pending: Arc<DashMap<Url, u64>>,
    /// The publish planner (backlog E6): every open document's last
    /// diagnostic groups, merged per target URI so shared dependencies show
    /// the union of their importers' views, and stale targets get explicit
    /// empties. Locked only around synchronous planning, never across an
    /// await.
    ///
    /// Every lock of this mutex — and of [`Backend::config`] below — RECOVERS
    /// from poisoning with `PoisonError::into_inner` (backlog E97, the tree's
    /// one posture). This server is the exact architecture the posture is for:
    /// `fenced` CATCHES a per-request panic so one bad request answers its
    /// fallback instead of locking the user out, and a propagated poison would
    /// undo that by wedging every later request on a mutex the caught panic left
    /// behind. This is the one shared structure here that a panic could leave
    /// *stale* rather than merely absent, which is why `plan_publish` drops the
    /// re-planning owner's entry before it computes the new one — see there.
    publish_state: Arc<std::sync::Mutex<PublishState>>,
    /// Line indices for files that are on disk and not buffered — `std`, and
    /// the workspace files a cross-file definition or reference reaches — so a
    /// query does not re-read and re-index one on every lookup. Each entry
    /// carries the [`FileStamp`] it was built from and is only served while the
    /// file still matches it (E112).
    line_indices: Arc<DashMap<PathBuf, (FileStamp, Arc<LineIndex>)>>,
    /// The client's feature settings, seeded from `initializationOptions` and
    /// updated live by `workspace/didChangeConfiguration`. Read per request
    /// (`inlay_hint`, `semantic_tokens_full`, …) so a toggle takes effect without
    /// re-registering capabilities. Poison-recovering like `publish_state`
    /// (E97); every write is a whole-value assignment, so a recovered guard
    /// reads either the old `Config` or the new one, never a mixture.
    config: Arc<std::sync::RwLock<Config>>,
    /// Whether the client can render snippet completions (`$1`/`${1:name}`
    /// tab-stops). Captured from `ClientCapabilities` at `initialize` (fixed for
    /// the session); when absent, call-shaped completions degrade to plain text
    /// (WO-3).
    snippet_support: Arc<AtomicBool>,
    /// The WORLD revision (E117): bumped by every notification that changes what
    /// an analysis would read — an open, an edit, a close, a save. An analysis
    /// is stamped with the value it started from
    /// ([`Document::stamp_analysis`]), which orders two results that finish out
    /// of order even when neither document's own text moved: a dependent's
    /// buffer is unchanged by an edit in the module it imports, so text equality
    /// cannot separate "read the module mid-edit" from "read it restored", and
    /// the loser used to publish last. That is the ghost diagnostic.
    revision: Arc<AtomicU64>,
    /// Serializes a publish's PLAN with its SEND (E117). The planner is a
    /// synchronous mutex, so plan order is well defined; without this gate the
    /// `publish_diagnostics` awaits of two publishes could still interleave and
    /// deliver the older plan last, which is the same ghost by a different
    /// route. Held across the sends and nothing else — the analyses themselves
    /// stay fully concurrent.
    publish_gate: Arc<tokio::sync::Mutex<()>>,
}

/// What a cached read of a file is only valid for: the file's length and its
/// modification time, as one comparable value (E112).
///
/// This is the whole invalidation rule for [`Backend::line_indices`]. It is
/// deliberately not a content hash — the point of the cache is to avoid reading
/// the file, and a `metadata` call is orders of magnitude cheaper than a read
/// plus an index build, so the cache keeps the win it exists for and stops
/// answering for text that is gone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct FileStamp {
    length: u64,
    modified: Option<std::time::SystemTime>,
}

/// The current stamp of the file at `path`, or `None` when it cannot be
/// stat-ed — which is the "do not cache this" answer: an entry with no stamp
/// could never be invalidated, which is the bug.
fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

/// Locate the `std` package directory: `$VILAN_STD`, else the nearest ancestor
/// of the document containing `vilan/std/vilan.toml` (a checkout — documents in
/// this repo resolve the working tree). `resolve_std` reads its `[library]`
/// manifest (or falls back to the layer convention if the path is a bare source
/// root).
fn discover_std_dir(start: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("VILAN_STD") {
        return PathBuf::from(path);
    }
    let mut directory = start.parent();
    while let Some(current) = directory {
        let candidate = current.join("vilan").join("std");
        if candidate.join("vilan.toml").is_file() {
            return candidate;
        }
        directory = current.parent();
    }
    // No ancestor carries a checkout — a project OUTSIDE the vilan repo (the
    // kolt shape, and every installed binary). Materialize the server's own
    // embedded std (real files, so definitions into std keep resolving); the
    // CLI does the same, so both tools see the identical std from any
    // directory. On a materialization failure (no writable home OR temp dir)
    // the path is left nonexistent and imports diagnose it.
    vilan_embedded_std::materialize()
        .unwrap_or_else(|_| PathBuf::from("<the embedded std could not be materialized>"))
}

#[cfg(test)]
mod std_discovery_tests {
    use super::discover_std_dir;

    #[test]
    fn a_document_outside_any_checkout_falls_back_to_the_embedded_std() {
        // A kolt-shaped path: no ancestor contains `vilan/std`. The fallback
        // must be the server's own materialized std — a real, complete package
        // directory that resolves from anywhere — not a compile-time path into
        // the machine the server happened to be built on.
        //
        // Built from the temp dir rather than written as a `/tmp/...` literal so
        // the "no ancestor is a checkout" premise holds on Windows too, where a
        // unix-absolute literal is a RELATIVE path and its ancestors are the
        // current directory's — i.e. this very checkout (windows-support.md §4).
        // The directories are never created, so nothing on the walk exists.
        let outside = std::env::temp_dir()
            .join("definitely")
            .join("not")
            .join("a")
            .join("checkout")
            .join("main.vl");
        assert!(!outside.exists(), "the fixture path must not exist");
        let discovered = discover_std_dir(&outside);
        assert!(
            discovered.is_absolute()
                && discovered.join("vilan.toml").is_file()
                && discovered.join("src/lib.vl").is_file(),
            "expected the materialized embedded std, got {discovered:?}"
        );
    }
}

#[cfg(test)]
mod config_tests {
    use super::{CompletionFunctionCall, Config};
    use serde_json::json;

    // Defaults preserve today's behavior: every provider on, full completion.
    #[test]
    fn defaults_preserve_todays_behavior() {
        let config = Config::default();
        assert!(config.inlay_hints_enabled);
        assert!(config.semantic_tokens_enabled);
        assert_eq!(
            config.completion_function_call,
            CompletionFunctionCall::Full
        );
    }

    // `initializationOptions` sends the bare `vilan` config object.
    #[test]
    fn parses_the_bare_vilan_object() {
        let config = Config::from_settings(&json!({
            "inlayHints": { "enabled": false },
            "semanticTokens": { "enabled": false },
            "completion": { "functionCall": "parensOnly" },
        }));
        assert!(!config.inlay_hints_enabled);
        assert!(!config.semantic_tokens_enabled);
        assert_eq!(
            config.completion_function_call,
            CompletionFunctionCall::ParensOnly
        );
    }

    // `didChangeConfiguration` wraps it as `{ "vilan": { … } }`; unspecified
    // fields keep their defaults.
    #[test]
    fn parses_the_wrapped_settings_and_keeps_unset_defaults() {
        let config = Config::from_settings(&json!({
            "vilan": {
                "inlayHints": { "enabled": false },
                "completion": { "functionCall": "none" },
            },
        }));
        assert!(!config.inlay_hints_enabled);
        assert!(config.semantic_tokens_enabled);
        assert_eq!(
            config.completion_function_call,
            CompletionFunctionCall::None
        );
    }

    // A partial, empty, or malformed payload never silently flips a provider off:
    // wrong types and unknown enum values fall back to the default.
    #[test]
    fn a_malformed_payload_keeps_defaults() {
        assert_eq!(Config::from_settings(&json!({})), Config::default());
        let config = Config::from_settings(&json!({
            "inlayHints": { "enabled": "yes" },
            "completion": { "functionCall": 3 },
        }));
        assert!(config.inlay_hints_enabled);
        assert_eq!(
            config.completion_function_call,
            CompletionFunctionCall::Full
        );
        let config = Config::from_settings(&json!({ "completion": { "functionCall": "wat" } }));
        assert_eq!(
            config.completion_function_call,
            CompletionFunctionCall::Full
        );
    }
}

#[cfg(test)]
mod manifest_routing_tests {
    use super::{ManifestDocument, is_manifest, to_manifest_item};
    use crate::manifest_completion;
    use tower_lsp::lsp_types::{CompletionItemKind, CompletionTextEdit, Url};

    // The routing question every notification asks. A manifest must never reach
    // `Document::analyze` (TOML through the vilan lexer is a wall of nonsense),
    // and a vilan file must never reach the manifest handler.
    #[test]
    fn only_a_file_named_vilan_toml_routes_to_the_manifest_handler() {
        let manifest = |path: &str| is_manifest(&Url::parse(path).expect("a url"));
        assert!(manifest("file:///work/app/vilan.toml"));
        assert!(manifest("file:///vilan.toml"));
        assert!(!manifest("file:///work/app/src/main.vl"));
        assert!(!manifest("file:///work/app/vilan.toml.bak"));
        assert!(!manifest("file:///work/app/Cargo.toml"));
        // A directory that merely CONTAINS the name is not the manifest.
        assert!(!manifest("file:///work/vilan.toml/notes.txt"));
    }

    // The completion an editor actually receives: an explicit text edit whose
    // range is the value token (quotes included), so applying it leaves valid
    // TOML rather than `""node"`.
    #[test]
    fn a_value_completion_arrives_as_a_text_edit_over_the_value_token() {
        let text = "[package]\nname = \"app\"\ntarget = \"\"\n";
        let offset = text.find("\"\"\n").expect("the empty value") + 1;
        let manifest = ManifestDocument::new(text.to_string());
        let item = manifest_completion::completions(&manifest.text, offset)
            .into_iter()
            .find(|item| item.label == "browser")
            .expect("`browser` is offered for a target");
        let converted = to_manifest_item(item, &manifest.line_index);
        assert_eq!(converted.kind, Some(CompletionItemKind::VALUE));
        let Some(CompletionTextEdit::Edit(edit)) = converted.text_edit else {
            panic!("a manifest completion carries its own edit");
        };
        assert_eq!(edit.new_text, "\"browser\"");
        // Line 2 (`target = …`), over both quotes.
        assert_eq!(edit.range.start.line, 2);
        assert_eq!(edit.range.start.character, 9);
        assert_eq!(edit.range.end.line, 2);
        assert_eq!(edit.range.end.character, 11);
    }

    #[test]
    fn a_key_completion_arrives_as_a_property() {
        let text = "[build]\npres\n";
        let offset = text.find("\npres").expect("the partial key") + 5;
        let manifest = ManifestDocument::new(text.to_string());
        let item = manifest_completion::completions(&manifest.text, offset)
            .into_iter()
            .find(|item| item.label == "preset")
            .expect("`preset` is offered in `[build]`");
        let converted = to_manifest_item(item, &manifest.line_index);
        assert_eq!(converted.kind, Some(CompletionItemKind::PROPERTY));
        let Some(CompletionTextEdit::Edit(edit)) = converted.text_edit else {
            panic!("a manifest completion carries its own edit");
        };
        assert_eq!(edit.new_text, "preset");
        assert_eq!(edit.range.start.character, 0);
        assert_eq!(edit.range.end.character, 4);
    }
}

#[cfg(test)]
mod completion_item_tests {
    use super::{CompletionFunctionCall, to_completion_item};
    use crate::line_index::LineIndex;
    use tower_lsp::lsp_types::{CompletionItemKind, Documentation, InsertTextFormat};
    use vilan_ide::{AutoImport, Completion, CompletionKind, SnippetInsertion};

    /// An empty-buffer index — every fixture below whose `needs_import` is
    /// `None` never consults it, so its content doesn't matter.
    fn blank_index() -> LineIndex {
        LineIndex::new("")
    }

    /// A function candidate as `Document` would hand it over: a full signature,
    /// a doc, and `call_parameters` naming the arguments (`None` = a bare name).
    fn function(call_parameters: Option<Vec<&str>>) -> Completion {
        Completion {
            label: "connect".to_string(),
            kind: CompletionKind::Function,
            detail: Some("fun connect(host: str, port: i32): Socket".to_string()),
            documentation: Some("Opens a connection.".to_string()),
            call_parameters: call_parameters
                .map(|names| names.into_iter().map(str::to_string).collect()),
            snippet: None,
            needs_import: None,
        }
    }

    // WO-3 `full`: each parameter becomes a named tab-stop, the cursor lands
    // after the call, and the signature-help popup is triggered.
    #[test]
    fn full_mode_inserts_named_parameter_placeholders() {
        let item = to_completion_item(
            function(Some(vec!["host", "port"])),
            CompletionFunctionCall::Full,
            true,
            &blank_index(),
        );
        assert_eq!(
            item.insert_text.as_deref(),
            Some("connect(${1:host}, ${2:port})$0")
        );
        assert_eq!(item.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert_eq!(
            item.command
                .as_ref()
                .map(|command| command.command.as_str()),
            Some("editor.action.triggerParameterHints"),
            "parameters present ⇒ trigger the hints popup"
        );
    }

    // WO-3 `parensOnly`: the parentheses are inserted with the cursor between
    // them, no named placeholders.
    #[test]
    fn parens_only_mode_positions_cursor_inside_parens() {
        let item = to_completion_item(
            function(Some(vec!["host", "port"])),
            CompletionFunctionCall::ParensOnly,
            true,
            &blank_index(),
        );
        assert_eq!(item.insert_text.as_deref(), Some("connect($0)"));
        assert_eq!(item.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert!(item.command.is_some(), "parameters present ⇒ trigger hints");
    }

    // WO-3 `none`: today's behavior — a bare name (no `insert_text`, so the
    // client inserts the label), and no hints command.
    #[test]
    fn none_mode_leaves_a_bare_name() {
        let item = to_completion_item(
            function(Some(vec!["host"])),
            CompletionFunctionCall::None,
            true,
            &blank_index(),
        );
        assert!(item.insert_text.is_none(), "the bare label is inserted");
        assert!(item.insert_text_format.is_none());
        assert!(item.command.is_none());
    }

    // WO-3: a zero-parameter callable inserts `name()$0` in BOTH call modes —
    // and, having no parameters, triggers no hints popup.
    #[test]
    fn zero_parameter_call_inserts_empty_parens_and_no_hints() {
        for mode in [
            CompletionFunctionCall::Full,
            CompletionFunctionCall::ParensOnly,
        ] {
            let item = to_completion_item(function(Some(vec![])), mode, true, &blank_index());
            assert_eq!(item.insert_text.as_deref(), Some("connect()$0"), "{mode:?}");
            assert_eq!(item.insert_text_format, Some(InsertTextFormat::SNIPPET));
            assert!(item.command.is_none(), "no parameters ⇒ no hints: {mode:?}");
        }
    }

    // WO-3: without client snippet support, a call-shaped insertion degrades to
    // plain `name()` (a snippet's tab-stops would otherwise show as literals).
    #[test]
    fn without_snippet_support_degrades_to_plain_parens() {
        let item = to_completion_item(
            function(Some(vec!["host", "port"])),
            CompletionFunctionCall::Full,
            false,
            &blank_index(),
        );
        assert_eq!(item.insert_text.as_deref(), Some("connect()"));
        assert_eq!(item.insert_text_format, Some(InsertTextFormat::PLAIN_TEXT));
    }

    // WO-3: a candidate with `call_parameters == None` (a non-callable, or one
    // the escape hatches suppressed) stays a bare name even in `full` mode.
    #[test]
    fn non_callable_stays_bare_in_full_mode() {
        let mut candidate = function(None);
        candidate.kind = CompletionKind::Struct;
        let item = to_completion_item(
            candidate,
            CompletionFunctionCall::Full,
            true,
            &blank_index(),
        );
        assert!(item.insert_text.is_none());
        assert!(item.command.is_none());
    }

    // WO-3: the popup always carries the signature `detail` and the `///`
    // documentation, independent of the insertion mode.
    #[test]
    fn detail_and_documentation_reach_the_item() {
        let item = to_completion_item(
            function(Some(vec!["host"])),
            CompletionFunctionCall::Full,
            true,
            &blank_index(),
        );
        assert_eq!(
            item.detail.as_deref(),
            Some("fun connect(host: str, port: i32): Socket")
        );
        assert!(
            matches!(item.documentation, Some(Documentation::String(doc)) if doc == "Opens a connection."),
            "the doc paragraph is attached"
        );
    }

    /// A construct-snippet candidate as `Document` hands it over (E14): the
    /// distinguishing label, a `Snippet` kind, and the tab-stopped body with its
    /// bare-keyword fallback.
    fn construct_snippet() -> Completion {
        Completion {
            label: "for … in { }".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("iterate over a collection".to_string()),
            documentation: None,
            call_parameters: None,
            snippet: Some(SnippetInsertion {
                body: "for ${1:item} in ${2:items} {\n\t$0\n}".to_string(),
                fallback: "for".to_string(),
            }),
            needs_import: None,
        }
    }

    /// An auto-import candidate as `Document::auto_import_completions` hands
    /// it over (E54c): labeled with its module and carrying the edit that
    /// adds it. `tier` mirrors [`crate::document::AutoImport::origin_tier`]
    /// (E59) — `2` is `std`'s.
    fn auto_import_candidate(label: &str, module_path: &[&str], tier: u8) -> Completion {
        Completion {
            label: label.to_string(),
            kind: CompletionKind::Struct,
            detail: None,
            documentation: None,
            call_parameters: None,
            snippet: None,
            needs_import: Some(AutoImport {
                module_path: module_path.iter().map(|part| part.to_string()).collect(),
                edit_span: vilan_core::Span { start: 0, end: 0 },
                edit_replacement: format!("import {}::{label};\n", module_path.join("::")),
                origin_tier: tier,
            }),
        }
    }

    // E14: a snippet-capable client gets the SNIPPET-iconed item with the
    // tab-stopped body inserted as a snippet, sorted after the entities (a `~`
    // prefix), and no parameter-hints command.
    #[test]
    fn construct_snippet_renders_as_a_snippet_item() {
        let item = to_completion_item(
            construct_snippet(),
            CompletionFunctionCall::Full,
            true,
            &blank_index(),
        );
        assert_eq!(item.kind, Some(CompletionItemKind::SNIPPET));
        assert_eq!(
            item.insert_text.as_deref(),
            Some("for ${1:item} in ${2:items} {\n\t$0\n}")
        );
        assert_eq!(item.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert!(
            item.sort_text
                .as_deref()
                .is_some_and(|s| s.starts_with('~')),
            "snippets sort after entities and keywords: {:?}",
            item.sort_text
        );
        assert!(
            item.command.is_none(),
            "a construct snippet triggers no hints"
        );
    }

    // E14: without client snippet support the body would surface as literal
    // `${1:…}` text, so the item degrades to inserting the bare keyword as plain
    // text — still iconed as a snippet.
    #[test]
    fn construct_snippet_without_snippet_support_falls_back_to_bare_keyword() {
        let item = to_completion_item(
            construct_snippet(),
            CompletionFunctionCall::Full,
            false,
            &blank_index(),
        );
        assert_eq!(
            item.insert_text.as_deref(),
            Some("for"),
            "the bare keyword, never a literal snippet body"
        );
        assert_eq!(item.insert_text_format, Some(InsertTextFormat::PLAIN_TEXT));
        assert_eq!(item.kind, Some(CompletionItemKind::SNIPPET));
    }

    // E14: the `vilan.completion.functionCall` setting shapes CALLS, not
    // construct snippets — a snippet renders identically under every mode.
    #[test]
    fn construct_snippet_ignores_the_function_call_mode() {
        for mode in [
            CompletionFunctionCall::None,
            CompletionFunctionCall::ParensOnly,
            CompletionFunctionCall::Full,
        ] {
            let item = to_completion_item(construct_snippet(), mode, true, &blank_index());
            assert_eq!(
                item.insert_text.as_deref(),
                Some("for ${1:item} in ${2:items} {\n\t$0\n}"),
                "{mode:?}"
            );
            assert_eq!(
                item.insert_text_format,
                Some(InsertTextFormat::SNIPPET),
                "{mode:?}"
            );
        }
    }

    // E54c: an auto-import candidate is labeled with its module, carries the
    // additional text edit that adds the import, and ranks below a plain
    // in-scope candidate (no `sort_text`, so the client falls back to its
    // label) — but above a construct snippet (`~`-prefixed).
    #[test]
    fn auto_import_candidate_is_labeled_and_carries_its_edit() {
        let index = LineIndex::new("fun main() {}\n");
        let item = to_completion_item(
            auto_import_candidate("Json", &["std", "json"], 2),
            CompletionFunctionCall::Full,
            true,
            &index,
        );
        assert_eq!(item.detail.as_deref(), Some("std::json"));
        let edits = item
            .additional_text_edits
            .expect("an auto-import candidate carries its edit");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "import std::json::Json;\n");
        assert_eq!(edits[0].range.start, edits[0].range.end, "a pure insertion");
        let sort_text = item.sort_text.expect("ranked below in-scope candidates");
        assert!(
            sort_text.starts_with('|'),
            "expected a `|`-prefixed sort_text, got {sort_text:?}"
        );
        // `|` (0x7C) sorts after any bare label (which has no `sort_text` and
        // so compares by its own alphanumeric-leading label) and before a
        // snippet's `~`-prefixed one (0x7E).
        assert!(
            sort_text.as_str() < "~",
            "{sort_text:?} must sort before snippets"
        );
        assert!(
            "connect" < sort_text.as_str(),
            "{sort_text:?} must sort after a bare in-scope label"
        );
    }

    // E59: `origin_tier` sorts BEFORE the label — a lower-tier candidate
    // (`pkg`, tier 0) ranks ahead of a higher-tier one (`std`, tier 2) even
    // when its label loses alphabetically. Plant-proof: swap the two tiers'
    // digits (or drop the digit and fall back to the old `|{label}` form)
    // and `"Apple"` (std) sorts first — this assertion goes red.
    #[test]
    fn origin_tier_outranks_the_label_within_the_auto_import_band() {
        let index = LineIndex::new("fun main() {}\n");
        let pkg_item = to_completion_item(
            auto_import_candidate("zebra", &["pkg", "helper"], 0),
            CompletionFunctionCall::Full,
            true,
            &index,
        );
        let std_item = to_completion_item(
            auto_import_candidate("Apple", &["std", "prelude"], 2),
            CompletionFunctionCall::Full,
            true,
            &index,
        );
        let pkg_sort_text = pkg_item.sort_text.expect("pkg candidate carries sort_text");
        let std_sort_text = std_item.sort_text.expect("std candidate carries sort_text");
        assert!(
            pkg_sort_text < std_sort_text,
            "pkg's `zebra` ({pkg_sort_text:?}) must outrank std's `Apple` \
             ({std_sort_text:?}) despite losing alphabetically"
        );
    }
}

#[cfg(test)]
mod code_action_tests {
    use super::organize_imports_requested;
    use tower_lsp::lsp_types::CodeActionKind;

    // Organize Imports is offered when unfiltered (the Source Action menu), for
    // its exact kind, and for the ancestor `source` kind (what
    // `codeActionsOnSave` requests).
    #[test]
    fn organize_is_offered_for_matching_and_ancestor_kinds() {
        assert!(organize_imports_requested(&None));
        assert!(organize_imports_requested(&Some(vec![
            CodeActionKind::SOURCE_ORGANIZE_IMPORTS
        ])));
        assert!(organize_imports_requested(&Some(vec![
            CodeActionKind::SOURCE
        ])));
    }

    // It is NOT offered for unrelated kinds, a sibling `source.*` kind, or an
    // empty filter.
    #[test]
    fn organize_is_not_offered_for_unrelated_kinds() {
        assert!(!organize_imports_requested(&Some(vec![
            CodeActionKind::QUICKFIX
        ])));
        assert!(!organize_imports_requested(&Some(vec![
            CodeActionKind::SOURCE_FIX_ALL
        ])));
        assert!(!organize_imports_requested(&Some(vec![])));
    }
}

#[cfg(test)]
mod semantic_token_encoding_tests {
    use super::{encode_semantic_tokens, token_delta};
    use crate::document::TokenKind;
    use crate::line_index::LineIndex;
    use vilan_core::Span;

    /// One classified span, as `Document::semantic_tokens` hands them over.
    fn token(range: std::ops::Range<usize>) -> (Span, TokenKind, u32) {
        (Span::from(range), TokenKind::Variable, 0)
    }

    // S6 / skew pin 5: positions AND deltas are UTF-16 code units. The line
    // here carries a 3-byte em-dash and a 4-byte astral emoji before the second
    // identifier, so the byte distance between the two tokens (24) and the
    // UTF-16 distance (20) disagree — a delta in bytes would drop the second
    // token four columns to the right of the word it highlights.
    #[test]
    fn token_deltas_are_utf16_units_not_byte_distances() {
        let text = "let title = \"— 😀\"; let value = 2;\n";
        let index = LineIndex::new(text);
        let title = text.find("title").expect("the first identifier");
        let value = text.find("value").expect("the second identifier");
        assert_eq!(value - title, 24, "the byte distance");
        let data = encode_semantic_tokens(
            &[
                token(title..title + "title".len()),
                token(value..value + "value".len()),
            ],
            &index,
        );
        assert_eq!((data[0].delta_line, data[0].delta_start), (0, 4));
        assert_eq!(
            (data[1].delta_line, data[1].delta_start),
            (0, 20),
            "…is 20 UTF-16 units: the em-dash counts 1 and the emoji 2",
        );
    }

    // S6, the `length` half. The classifier only ever hands over identifier
    // spans and identifiers are ASCII (`lexing.rs::is_identifier_start`), so
    // bytes and UTF-16 units agree for every token it can produce TODAY: this
    // pin is the guard for the day one of them covers text that isn't (a
    // string-interpolation hole, a comment, a wider identifier alphabet). A
    // byte length stretches the highlight over whatever follows the token.
    #[test]
    fn a_token_length_counts_utf16_units_not_bytes() {
        let text = "// — 😀\n";
        let index = LineIndex::new(text);
        let span = Span::from(3..text.find('\n').expect("a line end"));
        assert_eq!(span.into_range().len(), 8, "`— 😀` is eight bytes");
        let data = encode_semantic_tokens(&[(span, TokenKind::Variable, 0)], &index);
        assert_eq!(
            (data[0].delta_start, data[0].length),
            (3, 4),
            "…and four UTF-16 units (1 + 1 + 2)",
        );
    }

    // Pin 6: the delta rules themselves — a same-line token is relative to the
    // previous token's start, a token on a later line restarts from the line's
    // own column 0 (absolute), and the type/modifier bits ride through.
    #[test]
    fn deltas_are_relative_within_a_line_and_absolute_across_lines() {
        let text = "alpha beta\ngamma\n";
        let index = LineIndex::new(text);
        let data = encode_semantic_tokens(
            &[
                (Span::from(0..5), TokenKind::Variable, 0),
                (Span::from(6..10), TokenKind::Function, 1),
                (Span::from(11..16), TokenKind::Struct, 0),
            ],
            &index,
        );
        let shape: Vec<(u32, u32, u32, u32, u32)> = data
            .iter()
            .map(|item| {
                (
                    item.delta_line,
                    item.delta_start,
                    item.length,
                    item.token_type,
                    item.token_modifiers_bitset,
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (0, 0, 5, TokenKind::Variable as u32, 0),
                (0, 6, 4, TokenKind::Function as u32, 1),
                // Absolute, not `0 - 6`.
                (1, 0, 5, TokenKind::Struct as u32, 0),
            ],
        );
    }

    // A span that straddles a line boundary has no wire form (the encoding
    // carries one line delta and one width). It is dropped rather than sent at
    // a bogus width — and never panics, which is what an underflowing delta
    // would have done. Reachable only through a stale-index conversion, which
    // S1 now prevents; the guard stays because a language server must not abort.
    #[test]
    fn a_cross_line_span_is_dropped_and_its_neighbours_still_encode() {
        let text = "alpha\nbeta\n";
        let index = LineIndex::new(text);
        let data = encode_semantic_tokens(
            &[
                (Span::from(0..5), TokenKind::Variable, 0),
                // `alpha\nb` — straddles the newline.
                (Span::from(0..7), TokenKind::Variable, 0),
                (Span::from(6..10), TokenKind::Variable, 0),
            ],
            &index,
        );
        assert_eq!(data.len(), 2, "the straddling span is dropped");
        assert_eq!((data[1].delta_line, data[1].delta_start), (1, 0));
    }

    // --- B39b: the delta between two encoded streams ---
    //
    // One minimal edit, flat-integer units (five per token). Each case pins
    // a distinct shape: unchanged, middle replacement, append, truncation,
    // and both directions of empty.

    fn stream(widths: &[u32]) -> Vec<tower_lsp::lsp_types::SemanticToken> {
        widths
            .iter()
            .map(|width| tower_lsp::lsp_types::SemanticToken {
                delta_line: 1,
                delta_start: 0,
                length: *width,
                token_type: 0,
                token_modifiers_bitset: 0,
            })
            .collect()
    }

    #[test]
    fn an_unchanged_stream_deltas_to_zero_edits() {
        let tokens = stream(&[3, 4, 5]);
        assert!(token_delta(&tokens, &tokens).is_empty());
    }

    #[test]
    fn a_middle_change_replaces_only_the_middle() {
        let previous = stream(&[3, 4, 5]);
        let current = stream(&[3, 9, 5]);
        let edits = token_delta(&previous, &current);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].start, 5, "one common token = five flat integers");
        assert_eq!(edits[0].delete_count, 5, "one replaced token");
        assert_eq!(edits[0].data.as_ref().unwrap(), &stream(&[9]));
    }

    #[test]
    fn an_append_deletes_nothing() {
        let previous = stream(&[3, 4]);
        let current = stream(&[3, 4, 5]);
        let edits = token_delta(&previous, &current);
        assert_eq!(edits.len(), 1);
        assert_eq!((edits[0].start, edits[0].delete_count), (10, 0));
        assert_eq!(edits[0].data.as_ref().unwrap(), &stream(&[5]));
    }

    #[test]
    fn a_truncation_inserts_nothing() {
        let previous = stream(&[3, 4, 5]);
        let current = stream(&[3]);
        let edits = token_delta(&previous, &current);
        assert_eq!(edits.len(), 1);
        assert_eq!((edits[0].start, edits[0].delete_count), (5, 10));
        assert_eq!(edits[0].data.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn empty_streams_delta_in_both_directions() {
        let some = stream(&[3, 4]);
        let growing = token_delta(&[], &some);
        assert_eq!((growing[0].start, growing[0].delete_count), (0, 0));
        assert_eq!(growing[0].data.as_ref().unwrap(), &some);
        let shrinking = token_delta(&some, &[]);
        assert_eq!((shrinking[0].start, shrinking[0].delete_count), (0, 10));
        assert_eq!(shrinking[0].data.as_ref().unwrap().len(), 0);
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::{PauseAction, Refresh, pause_action, refresh_plan};

    // S5: a sweep that landed at least one analysis asks for BOTH providers,
    // once — the analyzed snapshot moved underneath answers the client is
    // already showing.
    #[test]
    fn a_landed_sweep_plans_one_refresh_pair() {
        assert_eq!(
            refresh_plan(true),
            &[Refresh::SemanticTokens, Refresh::InlayHints],
        );
    }

    // …and a sweep that landed nothing plans nothing: the answers out there
    // were computed against a snapshot that is still current, so a refresh
    // would only make the client re-ask for identical data.
    #[test]
    fn a_sweep_that_landed_nothing_plans_no_refresh() {
        assert!(refresh_plan(false).is_empty());
    }

    // The two ways a pause lands nothing. A newer edit supersedes this one (its
    // own pause will do the work), and a buffer byte-identical to the analyzed
    // text has nothing to analyze — notably the case where the user typed and
    // then undid it, which is also why staleness is text equality rather than a
    // flag (`Document::is_stale`).
    #[test]
    fn a_superseded_or_unchanged_pause_neither_analyzes_nor_refreshes() {
        assert_eq!(
            pause_action(Some(7), 6, Some(1), 2),
            PauseAction::Superseded,
        );
        assert_eq!(pause_action(None, 6, Some(1), 2), PauseAction::Superseded);
        assert_eq!(pause_action(Some(6), 6, Some(9), 9), PauseAction::Unchanged);
        assert_eq!(pause_action(Some(6), 6, Some(1), 2), PauseAction::Analyze);
        // A document with no analysis yet. `did_open` makes exactly one (E123)
        // and schedules its analysis in the same breath, so it reports the hash
        // of the text being analyzed rather than `None`; the skip must not
        // swallow the work if that ever changes.
        assert_eq!(pause_action(Some(6), 6, None, 2), PauseAction::Analyze);
    }
}

/// Analyze `text` as the document at `uri`, land the result on the open
/// document, and publish its diagnostics (grouped per file — backlog E1). The
/// analysis is CPU-bound, so it runs on a blocking thread to keep the async
/// runtime responsive. Returns whether the analysis landed (see [`land`]).
async fn analyze_and_publish(
    documents: &DashMap<Url, Document>,
    client: &Client,
    publish_state: &std::sync::Mutex<PublishState>,
    publish_gate: &tokio::sync::Mutex<()>,
    revision: &AtomicU64,
    uri: Url,
    text: String,
) -> bool {
    let path = uri.to_file_path().unwrap_or_default();
    let std_dir = discover_std_dir(&path);
    // E117: the world this analysis is about to read, sampled BEFORE it starts.
    // A later notification bumps the counter, so a result stamped lower is by
    // construction a view of an older world — whatever its own text says.
    let started_at = revision.load(Ordering::SeqCst);
    let mut analysis = match tokio::task::spawn_blocking(move || {
        Document::analyze(&text, &std_dir, &path)
    })
    .await
    {
        Ok(analysis) => analysis,
        Err(_) => return false,
    };
    analysis.stamp_analysis(started_at);
    if !land(documents, &uri, analysis) {
        return false;
    }
    publish_document(documents, client, publish_state, publish_gate, &uri).await;
    true
}

/// Land a completed analysis on the open document at `uri`
/// (`Document::adopt_analysis`). Returns whether it landed.
///
/// A result is dropped, not landed, in two cases:
///
/// - **The document is gone.** It was closed while the analysis ran; inserting
///   the result would resurrect a closed buffer — its diagnostics would
///   reappear with no document behind them and nothing left to clear them.
///   Only `did_open` ever puts a document INTO the map, so a missing entry can
///   only mean "closed".
/// - **The buffer moved on.** The analysis is of a text that is no longer the
///   live one. Two analyses of one document can be in flight at once (the
///   debounce generation is checked only before an analysis *starts*), and
///   they can finish in either order — adopting an older result on top of a
///   newer one would regress the analyzed snapshot and leave the document
///   stuck stale, with nothing scheduled to heal it until the next keystroke.
///   Dropping is always safe here: a live text this analysis doesn't match
///   implies a later `did_change` whose own debounced task (or an
///   already-landed fresher analysis) covers the buffer.
///
/// - **The world moved on** (E117). The analysis read an older world than the
///   one already adopted here: some file it loaded has been edited since it
///   started. Text equality cannot see this — it is the DEPENDENT's case, where
///   this document's own buffer never moved and both of its in-flight analyses
///   match it — so the [`Backend::revision`] stamp decides. Without it, the
///   analysis that read a module mid-edit could land (and publish) after the
///   one that read it restored, and the editor kept the error from a state the
///   user had already undone. Older strictly: an equal stamp is a second look
///   at the same world and lands normally.
///
/// So the analyzed snapshot only ever advances to *the* live text, never
/// sideways to a different stale one. `adopt_analysis` keeps its own
/// keep-the-live-side guard all the same — two independent layers: this one
/// never regresses the snapshot, that one never loses typed text.
///
/// Synchronous by construction: the map guard is taken and dropped here, never
/// held across the caller's `await`.
fn land(documents: &DashMap<Url, Document>, uri: &Url, analysis: Document) -> bool {
    let Some(mut document) = documents.get_mut(uri) else {
        return false;
    };
    if document.text != analysis.text {
        return false;
    }
    if analysis.analysis_revision() < document.analysis_revision() {
        return false;
    }
    document.adopt_analysis(analysis);
    true
}

/// Publish the stored document's diagnostics: the planner computes every
/// `(target, merged diagnostics)` action synchronously (the entry's own to
/// `uri`, each imported file's to *that file's* URI, stale targets cleared,
/// shared targets merged across owners — see `publish.rs`), and this sends
/// them.
async fn publish_document(
    documents: &DashMap<Url, Document>,
    client: &Client,
    publish_state: &std::sync::Mutex<PublishState>,
    publish_gate: &tokio::sync::Mutex<()>,
    uri: &Url,
) {
    // E117: plan and send as one step. The plan is already ordered (the planner
    // is a mutex, and it drops a superseded owner's plan outright); the gate is
    // what stops two publishes' `publish_diagnostics` awaits from interleaving
    // and delivering the older plan last.
    let _sending = publish_gate.lock().await;
    // Plan before the first await (neither the map guard nor the planner
    // lock may be held across one).
    let actions = {
        let Some(document) = documents.get(uri) else {
            return;
        };
        publish_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .plan_publish(uri, &document)
    };
    for (target, group) in actions {
        client.publish_diagnostics(target, group, None).await;
    }
}

/// Re-analyze the open documents that DEPEND on the changed file: an edit (or
/// save) of one file changes what its dependents see, so their diagnostics
/// must be recomputed — the stale-diagnostics half of backlog E1, now gated
/// on the real dependency edge (backlog B39a): a document whose last analysis
/// never loaded the changed file cannot see the edit, and re-analyzing every
/// open file anyway was the request path's largest fixed cost per typing
/// pause. Dependents' buffers didn't change, so this bypasses the
/// unchanged-text short-circuit deliberately. Conservative arms stay
/// conservative: a document with no program is swept (no recorded set), and
/// a changed URL that is not a file path sweeps everyone — both are the old
/// behavior, kept exactly where its reason still holds. Returns whether any
/// of them landed an analysis.
///
/// `recolored` widens the sweep to a whole package (E116): every open document
/// whose package root is at or under that path is swept, dependency edge or
/// not. The edge is the right gate for DIAGNOSTICS — a file that never loaded
/// the edited one cannot see the edit — and the wrong one for platform COLOR,
/// which is decided by which entry REACHES a file. That relation points the
/// other way: writing `import pkg::a` in the entry re-colors `a.vl`, and
/// `a.vl` depends on nothing, so the edge swept it never and the process
/// fallback stuck until the server restarted.
///
/// A saved MANIFEST arrives here the same way, and had the same hole from the
/// other end: `vilan.toml` is in no program's `canonical_sources`, so once the
/// sweep was gated on the edge (B39a) a manifest save re-analyzed nothing at
/// all — a target change, a new entry, a fixed dependency all sat there until
/// a restart. It passes its own directory, which every package root beneath it
/// is under.
async fn reanalyze_dependents(
    documents: &DashMap<Url, Document>,
    client: &Client,
    publish_state: &std::sync::Mutex<PublishState>,
    publish_gate: &tokio::sync::Mutex<()>,
    revision: &AtomicU64,
    changed: &Url,
    recolored: Option<&Path>,
) -> bool {
    let changed_path = changed.to_file_path().ok();
    let dependents: Vec<(Url, String)> = documents
        .iter()
        .filter(|entry| entry.key() != changed)
        .filter(|entry| {
            if let Some(recolored) = recolored
                && entry
                    .value()
                    .package_root()
                    .is_some_and(|root| root.starts_with(recolored))
            {
                return true;
            }
            match &changed_path {
                Some(path) => entry.value().depends_on(path),
                None => true,
            }
        })
        .map(|entry| (entry.key().clone(), entry.value().text.clone()))
        .collect();
    let mut landed = false;
    for (uri, text) in dependents {
        landed |= analyze_and_publish(
            documents,
            client,
            publish_state,
            publish_gate,
            revision,
            uri,
            text,
        )
        .await;
    }
    landed
}

/// How far the open document at `uri` reaches through its own package, and
/// which package that is (E116) — the two facts [`recolored_package`] compares
/// across a re-analysis. A closed or never-opened document reaches nothing.
///
/// Read synchronously; the guard is taken and dropped here, never held across
/// the caller's await.
fn package_reach(documents: &DashMap<Url, Document>, uri: &Url) -> Option<(u64, PathBuf)> {
    let document = documents.get(uri)?;
    let root = document.package_root()?;
    Some((document.package_graph_fingerprint(), root.to_path_buf()))
}

/// The package whose platform coloring a re-analysis invalidated, if any
/// (E116): its root when the set of package modules the edited file reaches
/// MOVED, `None` when the import graph is where it was.
///
/// The `pkg::` graph is what `platform_color::file_platforms` walks to decide
/// which entry reaches — and therefore colors — each file, so a change in it
/// can re-color files this one neither imports nor is imported by. Nothing
/// else in the file can: a body edit, a rename, a new `std` import all leave
/// the reach identical and skip the sweep.
///
/// Separated from its effects so the decision is testable without a server.
fn recolored_package(
    before: Option<(u64, PathBuf)>,
    after: Option<(u64, PathBuf)>,
) -> Option<PathBuf> {
    let (after_reach, root) = after?;
    match before {
        // The same package, reaching the same modules: nothing to re-color.
        Some((before_reach, before_root)) if before_reach == after_reach && before_root == root => {
            None
        }
        _ => Some(root),
    }
}

/// One thing the server asks the client to re-request after a sweep of
/// analyses lands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Refresh {
    SemanticTokens,
    InlayHints,
}

/// What a completed sweep owes the client, as data — the publish planner's
/// pattern (backlog E6): the decision is computed here and unit-tested, and
/// [`send_refreshes`] merely transmits it.
///
/// A sweep that landed at least one analysis has moved the analyzed snapshot
/// underneath answers the client is already showing, so both providers are
/// asked for once — per sweep, not per document. A sweep that landed nothing
/// (superseded, closed, or the buffer was byte-identical to what we last
/// analyzed) owes nothing: the answers out there were computed against a
/// snapshot that is still current.
fn refresh_plan(landed: bool) -> &'static [Refresh] {
    if landed {
        &[Refresh::SemanticTokens, Refresh::InlayHints]
    } else {
        &[]
    }
}

/// Send a [`refresh_plan`]. Errors are ignored: a client that doesn't support
/// refresh answers with one, and that is a no-op, not a failure.
async fn send_refreshes(client: &Client, plan: &[Refresh]) {
    for refresh in plan {
        let _ = match refresh {
            Refresh::SemanticTokens => client.semantic_tokens_refresh().await,
            Refresh::InlayHints => client.inlay_hint_refresh().await,
        };
    }
}

/// What a debounced pause does when it wakes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PauseAction {
    /// A newer edit (or a close) arrived while we slept — that one will run.
    Superseded,
    /// The buffer is byte-for-byte what we last analyzed. Nothing to analyze,
    /// and nothing to refresh: every answer already out there is still right.
    Unchanged,
    /// Re-analyze and sweep the dependents.
    Analyze,
}

/// The pause decision, separated from its effects so both skips are testable.
fn pause_action(
    pending_generation: Option<u64>,
    this_generation: u64,
    analyzed_hash: Option<u64>,
    buffer_hash: u64,
) -> PauseAction {
    if pending_generation != Some(this_generation) {
        PauseAction::Superseded
    } else if analyzed_hash == Some(buffer_hash) {
        PauseAction::Unchanged
    } else {
        PauseAction::Analyze
    }
}

impl Backend {
    /// Runs a request handler's synchronous body under a panic fence (B40).
    /// A panic in a handler used to unwind through the async runtime and
    /// abort the whole server — exit 101, and after five crashes in three
    /// minutes the client stops restarting it, so one bad request locked the
    /// user out of every LSP feature. Here it degrades to `fallback`, loudly:
    /// the default panic hook has already put the payload and location on
    /// stderr (the extension's output channel), and an ERROR log names the
    /// handler. Every query handler's body is `.await`-free — pure work over
    /// a snapshot guard — which is what lets one synchronous seam cover them
    /// all (a DashMap guard just drops on unwind; the two `std::sync` locks
    /// are poison-tolerant so a caught panic cannot convert into a
    /// panic-on-every-later-request loop).
    fn fenced<R>(&self, request: &'static str, fallback: R, work: impl FnOnce() -> R) -> R {
        let started = std::time::Instant::now();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(test)]
            panic_fence_tests::maybe_inject(request);
            work()
        }));
        // E106: the fence is the ONE synchronous seam every request already
        // passes through, which makes it the place to time them all without
        // touching a handler. A panicked request is timed too — the time it
        // burned before unwinding is time the user waited.
        self.trace_request(request, started.elapsed().as_millis());
        match caught {
            Ok(answer) => answer,
            Err(payload) => {
                let text = format!(
                    "the {request} handler panicked: {}; answered with its fallback (this is a vilan-lsp bug; details on stderr)",
                    panic_message(payload.as_ref())
                );
                eprintln!("vilan-lsp: {text}");
                let client = self.client.clone();
                tokio::spawn(async move {
                    client.log_message(MessageType::ERROR, text).await;
                });
                fallback
            }
        }
    }

    /// E106: fold one request's duration into the session tally, and put the
    /// trace's own verdict on the client's output channel.
    ///
    /// Nothing is logged for an ordinary request, so a healthy session is
    /// silent; a slow one is named with its duration, and every
    /// [`session_trace::SUMMARY_EVERY_REQUESTS`] requests the whole profile goes
    /// out together with the server's retained-state cardinalities. Those counts
    /// are the growth evidence: one that climbs while `documents` does not is a
    /// leak with a name, which is what the item asks for before any reclaim is
    /// designed.
    ///
    /// The send is spawned like the panic fence's, and guarded on a runtime
    /// actually being present — the tally itself is pure and must stay usable
    /// from a plain synchronous caller.
    fn trace_request(&self, request: &'static str, elapsed_ms: u128) {
        let text = match session_trace::record(request, elapsed_ms) {
            session_trace::TraceEvent::Quiet => return,
            session_trace::TraceEvent::Slow(line) => line,
            session_trace::TraceEvent::Summarize => {
                session_trace::summary(session_trace::StateSizes {
                    documents: self.documents.len(),
                    semantic_token_cache: self.semantic_token_cache.len(),
                    manifests: self.manifests.len(),
                    pending: self.pending.len(),
                    line_indices: self.line_indices.len(),
                })
            }
        };
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let client = self.client.clone();
        tokio::spawn(async move {
            client.log_message(MessageType::INFO, text).await;
        });
    }

    /// Schedule a debounced re-analysis. A burst of edits collapses to a single
    /// analysis once typing pauses, and an edit that leaves the buffer unchanged
    /// is skipped entirely.
    fn on_change(&self, uri: Url, text: String) {
        let generation = {
            let mut entry = self.pending.entry(uri.clone()).or_insert(0);
            *entry += 1;
            *entry
        };
        let documents = Arc::clone(&self.documents);
        let pending = Arc::clone(&self.pending);
        let publish_state = Arc::clone(&self.publish_state);
        let publish_gate = Arc::clone(&self.publish_gate);
        let revision = Arc::clone(&self.revision);
        let client = self.client.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
            // Read both facts synchronously (no map guard may cross an await),
            // then decide.
            let current_generation = pending.get(&uri).map(|current| *current);
            let analyzed_hash = documents.get(&uri).map(|document| document.text_hash);
            match pause_action(
                current_generation,
                generation,
                analyzed_hash,
                hash_text(&text),
            ) {
                PauseAction::Superseded | PauseAction::Unchanged => return,
                PauseAction::Analyze => {}
            }
            let reach_before = package_reach(&documents, &uri);
            let landed = analyze_and_publish(
                &documents,
                &client,
                &publish_state,
                &publish_gate,
                &revision,
                uri.clone(),
                text,
            )
            .await;
            // The edit may change what other open files see (they import this
            // one, or a file it re-exports) — bring their diagnostics up to date.
            let recolored = recolored_package(reach_before, package_reach(&documents, &uri));
            let dependents_landed = reanalyze_dependents(
                &documents,
                &client,
                &publish_state,
                &publish_gate,
                &revision,
                &uri,
                recolored.as_deref(),
            )
            .await;
            // The analyzed snapshot moved under the client's highlighting and
            // hints; ask for them again (S5). Every guard is long dropped here.
            send_refreshes(&client, refresh_plan(landed || dependents_landed)).await;
        });
    }

    /// The line index for a file another source's span points into, cached by
    /// path so a cross-file query doesn't re-read and re-index on every lookup.
    ///
    /// A path with a buffer registered is indexed fresh every time and never
    /// stored: its text is one keystroke old, so a stored index would misplace
    /// every range it converts from the next edit onward.
    ///
    /// Every other entry is validated against the file's [`FileStamp`] (E112).
    /// The cache used to have no invalidation at all, documented as safe
    /// because it was written for `std`, "whose files genuinely do not change".
    /// That was never a property of the KEY — it is a property of std, and the
    /// map holds whatever a cross-file query asks for. A workspace file is
    /// exempt from the cache only while it is buffered, so CLOSING a document
    /// makes it cacheable; a file cached before it was ever opened kept its
    /// pre-edit index across the whole open/edit/save/close cycle, and every
    /// later reference into it converted through the wrong line breaks. So the
    /// invariant is made true instead of assumed: a hit must match the file's
    /// current length and modification time, and anything else re-reads. That
    /// also covers what no did-close hook could — a change made outside the
    /// editor entirely, a `git checkout` or a generator's rewrite.
    fn line_index_for(&self, path: &Path) -> Option<Arc<LineIndex>> {
        let buffered = vilan_core::analyzer::document_overlay_contains(path);
        let stamp = if buffered { None } else { file_stamp(path) };
        if let Some(stamp) = stamp
            && let Some(cached) = self.line_indices.get(path)
            && cached.value().0 == stamp
        {
            return Some(Arc::clone(&cached.value().1));
        }
        // A disk read is BOM-stripped, matching the analyzer's read of the same
        // file (windows-support.md §2); a buffer comes back verbatim. Either
        // way this index and the spans it converts index the same text the
        // analyzer saw, which is the whole point.
        let text = vilan_core::util::read_source(path).ok()?;
        let line_index = Arc::new(LineIndex::new(&text));
        // Stamped with what was read BEFORE the read, so a file that changed
        // during it looks stale on the next lookup and is read again — the
        // conservative direction, and the only one that cannot answer wrong.
        if let Some(stamp) = stamp {
            self.line_indices
                .insert(path.to_path_buf(), (stamp, Arc::clone(&line_index)));
        }
        Some(line_index)
    }

    /// Convert a `(source, span)` from analysis into an LSP `Location`. The entry
    /// file uses the open document's line index; a `std` file uses its cached one.
    fn location_for(
        &self,
        document: &Document,
        doc_uri: &Url,
        source: SourceId,
        span: Span,
    ) -> Option<Location> {
        if source == SourceId(0) {
            return Some(Location {
                uri: doc_uri.clone(),
                // A program span indexes the ANALYZED text (S1).
                range: document.analyzed_range(&span),
            });
        }
        let program = document.program.as_ref()?;
        let path = program.source_path(source)?;
        let line_index = self.line_index_for(path)?;
        let uri = Url::from_file_path(path).ok()?;
        Some(Location {
            uri,
            range: line_index.range(&span),
        })
    }

    /// Convert a cross-program `(canonical path, span)` — the coordinates the
    /// document layer's cross-document union answers in (kolt.local 034) —
    /// into an LSP `Location`.
    ///
    /// A path that is an open document's ENTRY converts through that
    /// document's analyzed index and answers with the URI the client opened it
    /// under: the span came from an analysis of exactly that text (S1). Any
    /// other file converts through the session line-index cache, exactly as
    /// [`Backend::location_for`] always has for a non-entry source.
    fn location_for_path(
        &self,
        open: &[dashmap::mapref::multiple::RefMulti<'_, Url, Document>],
        path: &Path,
        span: Span,
    ) -> Option<Location> {
        for entry in open {
            if entry.value().entry_path() == Some(path) {
                return Some(Location {
                    uri: entry.key().clone(),
                    range: entry.value().analyzed_range(&span),
                });
            }
        }
        let line_index = self.line_index_for(path)?;
        let uri = Url::from_file_path(path).ok()?;
        Some(Location {
            uri,
            range: line_index.range(&span),
        })
    }
}

/// What the server advertises at `initialize` — every provider it answers
/// for, and by omission every one it does not. A pure value, so the book's
/// editor page can be held to it (`book_sync.rs`): the page's "what it gives
/// you" and "what it does not have" are claims about exactly this struct.
fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                // B39c: the client sends ranged edits, not the
                // whole buffer per keystroke.
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                ..Default::default()
            },
        )),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        // Element syntax S5: editing one tag name renames its pair.
        linked_editing_range_provider: Some(LinkedEditingRangeServerCapabilities::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            // `.` and `:` (the second `:` of `::`) re-trigger completion so
            // member/path candidates appear without a manual invoke.
            trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
            ..Default::default()
        }),
        inlay_hint_provider: Some(OneOf::Left(true)),
        // E2: precision highlighting from the analyzed program. The
        // legend is index-aligned with `document::TokenKind`.
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: crate::document::TOKEN_TYPES
                        .iter()
                        .map(|name| SemanticTokenType::new(name))
                        .collect(),
                    token_modifiers: crate::document::TOKEN_MODIFIERS
                        .iter()
                        .map(|name| SemanticTokenModifier::new(name))
                        .collect(),
                },
                full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                range: Some(true),
                ..Default::default()
            },
        )),
        // WO-2: the "Organize Imports" source action (sort + prune).
        // E54: QUICKFIX (add-import, and E58's field-name rename) and
        // the "add all missing imports" source action.
        // css-block S5: REFACTOR_REWRITE, the server's first — the
        // block/chain conversion is not diagnostic-driven, so it needs a kind
        // of its own for the editor to ask for it.
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![
                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                CodeActionKind::QUICKFIX,
                CodeActionKind::REFACTOR_REWRITE,
                fix_all_imports_kind(),
            ]),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Whether the formatting handler must decline `path` whole: a file under a
/// declared `generated` root is a product, and the editor holds to that as
/// firmly as the terminal does (`build-hooks.md` §12.4). This is the path the
/// rule most has to reach — format-on-save fires on a file the developer
/// merely opened to read, so the fmt↔hook loop §12.1 describes would
/// otherwise run without anyone having typed a command. Same predicate as
/// `vilan fmt`'s — one rule, one implementation, or the editor's answer
/// drifts from the terminal's.
fn formatting_declined(path: &std::path::Path) -> bool {
    vilan_core::manifest::generated_root_covering(path).is_some()
}

#[cfg(test)]
mod formatting_gate_tests {
    use super::formatting_declined;

    // The editor half of the generated-root exclusion (build-hooks.md §12.4):
    // format-on-save reaches a file by its exact path and nothing else, so the
    // handler's gate is the terminal's predicate behind this one seam. Pinned
    // against a real manifest on disk, the way the CLI pins do for
    // `vilan fmt` — decline the product, format the source beside it.
    #[test]
    fn the_generated_root_declines_the_product_and_not_its_neighbour() {
        let dir = std::env::temp_dir().join(format!(
            "vilan_lsp_genroot_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let generated = dir.join("src/icons");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(
            dir.join("vilan.toml"),
            "[package]\nname = \"gated\"\ngenerated = \"src/icons\"\n",
        )
        .unwrap();
        let product = generated.join("lib.vl");
        let source = dir.join("src/main.vl");
        std::fs::write(&product, "fun generated(): i32 { 41 }\n").unwrap();
        std::fs::write(&source, "fun main() {}\n").unwrap();
        assert!(
            formatting_declined(&product),
            "a product under the declared root is declined whole"
        );
        assert!(
            !formatting_declined(&source),
            "an ordinary source beside the root still formats"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // G17, the editor half. A `generated` root declared through a symlink is
    // the case format-on-save most has to get right: the CLI can at least be
    // re-run, while this handler fires on a file the developer merely opened.
    // The predicate used to climb the file's CANONICAL path only, so a link out
    // of the package left the manifest unfindable and the handler formatted the
    // product — §12.1's loop, started by nobody typing a command.
    //
    // `cfg(unix)`: creating a symlink needs a privilege Windows does not grant
    // by default (audit run 7 owns the Windows half). The FIX is platform-neutral.
    #[cfg(unix)]
    #[test]
    fn a_generated_root_declared_through_a_symlink_is_declined_too() {
        let dir = std::env::temp_dir().join(format!(
            "vilan_lsp_genlink_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let package = dir.join("package");
        let outside = dir.join("outside/icons");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(
            package.join("vilan.toml"),
            "[package]\nname = \"gated\"\ngenerated = \"src/icons\"\n",
        )
        .unwrap();
        std::os::unix::fs::symlink("../../outside/icons", package.join("src/icons")).unwrap();
        let product = package.join("src/icons/lib.vl");
        let source = package.join("src/main.vl");
        std::fs::write(&product, "fun generated(): i32 { 41 }\n").unwrap();
        std::fs::write(&source, "fun main() {}\n").unwrap();
        assert!(
            formatting_declined(&product),
            "a product behind a symlinked root is declined whole"
        );
        assert!(
            !formatting_declined(&source),
            "and the hand-written module beside it still formats"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.fenced("initialize", Err(JsonRpcError::internal_error()), || {
            // Seed the feature settings from the client's `initializationOptions`
            // (the extension sends the `vilan` config object); later changes arrive
            // via `did_change_configuration`.
            if let Some(options) = &params.initialization_options {
                *self
                    .config
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Config::from_settings(options);
            }
            // Snippet completions (tab-stop placeholders) need the client to opt in
            // via `completionItem.snippetSupport`; without it, a call-shaped
            // completion degrades to plain text. This is fixed for the session.
            let snippet_support = params
                .capabilities
                .text_document
                .as_ref()
                .and_then(|text_document| text_document.completion.as_ref())
                .and_then(|completion| completion.completion_item.as_ref())
                .and_then(|completion_item| completion_item.snippet_support)
                .unwrap_or(false);
            self.snippet_support
                .store(snippet_support, Ordering::Relaxed);
            Ok(InitializeResult {
                capabilities: server_capabilities(),
                server_info: Some(ServerInfo {
                    name: "vilan-lsp".to_string(),
                    version: None,
                }),
            })
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "vilan-lsp initialized")
            .await;
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        // Our client pushes `{ "vilan": { … } }` on a relevant change; re-parse
        // and replace (providers read the config per request, so a toggle is live
        // without re-registration). Ignore a payload without the `vilan` section
        // — the language client also emits a bare `{ settings: null }` on any
        // config change, which must NOT reset our settings to their defaults.
        if params.settings.get("vilan").is_some() {
            *self
                .config
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Config::from_settings(&params.settings);
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        // Insert the document before the first `.await`, so a query that
        // arrives right after open still finds it — and so `land`'s "a missing
        // entry can only mean closed" stays true — then SCHEDULE the first
        // analysis the way an edit's is scheduled.
        //
        // E123: this used to call `Document::analyze` right here, on the async
        // handler. That call joins a 128 MiB analysis thread, so opening
        // kolt's `views.vl` parked a tokio worker for the whole 1.1 s first
        // analysis and every other request on that worker waited behind it
        // (`proposal/editor-latency.md` §1.6; the session trace's "slow
        // request: didOpen took 1112 ms"). The work is unchanged and the
        // stamping is E117's; only its thread is different — `spawn_blocking`,
        // like every other analysis in this file.
        let uri = params.text_document.uri;
        // The synchronous prefix fences (B40) and hands back the text to
        // analyze, or nothing when there is no analysis to schedule. A panicked
        // open publishes nothing — the map entry it failed to make is what
        // "open" means everywhere else.
        let opened = self.fenced("didOpen", None, || {
            // A manifest is not a vilan source file: it feeds completion and
            // nothing else. It is deliberately NOT registered as a document
            // overlay either — project resolution reads `vilan.toml` from disk, so
            // an unsaved manifest edit takes effect on save (which re-analyzes
            // every open document through `did_save`).
            if is_manifest(&uri) {
                self.manifests.insert(
                    uri.clone(),
                    ManifestDocument::new(params.text_document.text),
                );
                return None;
            }
            let path = uri.to_file_path().unwrap_or_default();
            // Register the buffer so OTHER documents' analyses load this one's
            // live content instead of the file on disk (backlog E6).
            vilan_core::analyzer::set_document_overlay(
                &path,
                Some(params.text_document.text.clone()),
            );
            // The overlay just changed what every analysis reads (E117). The
            // analysis scheduled below samples the counter AFTER this bump, so
            // it is stamped with the world it actually reads.
            self.revision.fetch_add(1, Ordering::SeqCst);
            // The ONLY place a document enters the map. Every later analysis lands
            // by merge onto what is here (`land`), which is what lets a result
            // arriving after `did_close` be dropped instead of resurrecting the
            // file: a missing entry can only mean "closed", never "not opened yet".
            // It holds the buffer and no analysis yet — the state the debounce
            // window has always had between an edit and the analysis that
            // answers it, and every query handler already reads it.
            self.documents.insert(
                uri.clone(),
                Document::unanalyzed(&params.text_document.text),
            );
            Some(params.text_document.text)
        });
        let Some(text) = opened else {
            return;
        };
        // Spawned, not awaited: `did_change` returns the instant it has
        // scheduled its analysis, and an open must too — the notification
        // handler is on the same runtime every request is served from.
        let documents = Arc::clone(&self.documents);
        let publish_state = Arc::clone(&self.publish_state);
        let publish_gate = Arc::clone(&self.publish_gate);
        let revision = Arc::clone(&self.revision);
        let client = self.client.clone();
        tokio::spawn(async move {
            // The shared path: `spawn_blocking`, stamped with the world it
            // read, landed only if it is still the newest view of the live
            // text (E117), then published.
            let landed = analyze_and_publish(
                &documents,
                &client,
                &publish_state,
                &publish_gate,
                &revision,
                uri,
                text,
            )
            .await;
            // The analyzed snapshot moved under whatever the client asked for
            // in the meantime — it opened the file and immediately asked for
            // tokens and hints over a document that had none. Ask it to ask
            // again (S5); this is what makes the empty first answer transient.
            send_refreshes(&client, refresh_plan(landed)).await;
        });
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.fenced("did_change", (), || {
            if params.content_changes.is_empty() {
                return;
            }
            let uri = params.text_document.uri;
            if is_manifest(&uri) {
                // Manifests fold the same ordered-events contract over their
                // own stored text; they keep no edit log (nothing maps).
                let mut text = self
                    .manifests
                    .get(&uri)
                    .map(|manifest| manifest.text.clone())
                    .unwrap_or_default();
                for change in &params.content_changes {
                    match change.range {
                        None => text = change.text.clone(),
                        Some(range) => {
                            let index = LineIndex::new(&text);
                            let start = index.offset(range.start);
                            let end = index.offset(range.end).max(start);
                            text.replace_range(start..end, &change.text);
                        }
                    }
                }
                self.manifests.insert(uri, ManifestDocument::new(text));
                return;
            }
            // Apply the edits to the open document immediately — in order,
            // each against the text as already edited (the incremental-sync
            // contract) — so a completion request arriving before the
            // debounced re-analysis still sees the just-typed character.
            // A document the protocol never opened has no base to splice
            // into; ranged events for it are dropped by the same guard.
            let text = {
                let Some(mut document) = self.documents.get_mut(&uri) else {
                    return;
                };
                for change in &params.content_changes {
                    document.apply_change(change.range, &change.text);
                }
                document.text.clone()
            };
            // The overlay updates immediately (pre-debounce), so any analysis
            // that runs meanwhile — a dependent's, this one's — sees the edit.
            if let Ok(path) = uri.to_file_path() {
                vilan_core::analyzer::set_document_overlay(&path, Some(text.clone()));
            }
            // The world every analysis reads has moved (E117) — bump BEFORE the
            // debounced task samples it, so an analysis already in flight is
            // stamped with the world it actually read and this edit's own
            // analysis is stamped strictly higher.
            self.revision.fetch_add(1, Ordering::SeqCst);
            self.on_change(uri, text);
        })
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // A save changes what OTHER documents' analyses read from disk (module
        // loading is disk-backed), so re-analyze every open document.
        let saved = params.text_document.uri;
        // A save changes what a disk read answers, so it moves the world too
        // (E117) — a manifest save especially, which re-colors every open file.
        self.revision.fetch_add(1, Ordering::SeqCst);
        // E116: a saved `vilan.toml` is the coloring input itself — its target,
        // its entries, its `default-entry` all decide what every file under it
        // analyzes as — and it is in no program's `canonical_sources`, so the
        // dependency edge alone finds nothing to sweep. Its own directory
        // stands for the packages beneath it.
        let saved_manifest_directory = is_manifest(&saved)
            .then(|| saved.to_file_path().ok())
            .flatten()
            .and_then(|path| path.parent().map(vilan_core::util::canonical_path));
        let reach_before = package_reach(&self.documents, &saved);
        // `.map` consumes the map guard inside the closure, so nothing is held
        // across the awaits below (which take the same key for writing).
        let mut landed = false;
        if let Some((uri, text)) = self
            .documents
            .get(&saved)
            .map(|document| (saved.clone(), document.text.clone()))
        {
            landed = analyze_and_publish(
                &self.documents,
                &self.client,
                &self.publish_state,
                &self.publish_gate,
                &self.revision,
                uri,
                text,
            )
            .await;
        }
        let recolored = saved_manifest_directory
            .or_else(|| recolored_package(reach_before, package_reach(&self.documents, &saved)));
        landed |= reanalyze_dependents(
            &self.documents,
            &self.client,
            &self.publish_state,
            &self.publish_gate,
            &self.revision,
            &saved,
            recolored.as_deref(),
        )
        .await;
        // Same sweep rule as a typing pause (S5).
        send_refreshes(&self.client, refresh_plan(landed)).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        // A closed manifest publishes nothing and clears nothing: the
        // diagnostic ON it belongs to the packages that read it, which are
        // still open (see `document::ManifestProblem`). Falling through would
        // reach the unconditional clear below and wipe a live diagnostic.
        if is_manifest(&uri) {
            self.manifests.remove(&uri);
            return;
        }
        // Disk truth returns for other documents' analyses.
        if let Ok(path) = uri.to_file_path() {
            vilan_core::analyzer::set_document_overlay(&path, None);
        }
        // Dropping the overlay changes what every other analysis reads (E117).
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.documents.remove(&uri);
        self.semantic_token_cache.remove(&uri);
        // Drop the edit generation so any in-flight debounced analysis bails.
        self.pending.remove(&uri);
        // Clear this document's diagnostics AND the ones it published onto
        // other files — each target republishes as the remaining owners'
        // merged view (empty where this was the only contributor). Under the
        // same plan-with-send gate every other publish takes (E117), so a
        // concurrent analysis's sends cannot land in the middle of the clear.
        let _sending = self.publish_gate.lock().await;
        let actions = self
            .publish_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .plan_close(&uri);
        for (target, group) in actions {
            self.client.publish_diagnostics(target, group, None).await;
        }
        // A document that never analyzed (open failed) still clears.
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        self.fenced("inlay_hint", Ok(None), || {
            // `vilan.inlayHints.enabled` gates the provider server-side.
            if !self
                .config
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .inlay_hints_enabled
            {
                return Ok(None);
            }
            let uri = params.text_document.uri;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            let range = params.range;
            let hints = document
                // E121 (Q1/Q4): the landed hints re-mapped through the
                // two-sided anchor, WITHHELD inside the edit window — a hint on
                // the line you are typing is the most likely to be wrong and
                // the least useful, and its absence there is invisible because
                // it was about to move anyway. A hint outside the window sits
                // on byte-identical text, so its position is exact.
                //
                // That exactness is what retires the analyzed/live index dance
                // this filter used to need: the offsets are already live-space,
                // so one index answers both the hint's position and the
                // viewport compare, and there is no approximation left to
                // fall back to.
                .keystroke_hints(false)
                .into_iter()
                .filter_map(|(offset, label)| {
                    let position = document.line_index.position(offset);
                    let visible = position >= range.start && position <= range.end;
                    visible.then_some(InlayHint {
                        position,
                        label: InlayHintLabel::String(label),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(false),
                        padding_right: Some(false),
                        data: None,
                    })
                })
                .collect::<Vec<_>>();
            Ok(Some(hints))
        })
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.fenced("semantic_tokens_full", Ok(None), || {
            // `vilan.semanticTokens.enabled` gates the provider server-side; when off,
            // the editor falls back to its TextMate grammar.
            if !self
                .config
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .semantic_tokens_enabled
            {
                return Ok(None);
            }
            let uri = params.text_document.uri;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // E121's keystroke path: the LANDED stream re-mapped through the
            // two-sided anchor plus the edit window painted from syntax, in
            // LIVE coordinates — so the encode goes through the LIVE index, not
            // the analyzed one. That index switch IS the change: an answer that
            // describes the buffer on screen has to be positioned against it.
            let data =
                encode_semantic_tokens(&document.keystroke_tokens(false), &document.line_index);
            drop(document);
            let id = fresh_result_id();
            self.semantic_token_cache
                .insert(uri, (id.clone(), data.clone()));
            Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: Some(id),
                data,
            })))
        })
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        self.fenced("semantic_tokens_full_delta", Ok(None), || {
            if !self
                .config
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .semantic_tokens_enabled
            {
                return Ok(None);
            }
            let uri = params.text_document.uri;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // The same stream `semantic_tokens_full` answers with (E121), or
            // the delta chain would compare two different pictures.
            let data =
                encode_semantic_tokens(&document.keystroke_tokens(false), &document.line_index);
            drop(document);
            let id = fresh_result_id();
            // Swap the baseline for the new stream in one motion; the OLD
            // entry decides whether a delta is even possible.
            let previous = self
                .semantic_token_cache
                .insert(uri, (id.clone(), data.clone()));
            match previous {
                // The client's baseline is the one we remember: answer the
                // difference — zero edits when nothing moved.
                Some((previous_id, previous_data)) if previous_id == params.previous_result_id => {
                    Ok(Some(SemanticTokensFullDeltaResult::TokensDelta(
                        SemanticTokensDelta {
                            result_id: Some(id),
                            edits: token_delta(&previous_data, &data),
                        },
                    )))
                }
                // An unknown baseline (restart, eviction, a response the
                // client never saw): a full stream re-synchronizes.
                _ => Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: Some(id),
                        data,
                    },
                ))),
            }
        })
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        self.fenced("semantic_tokens_range", Ok(None), || {
            if !self
                .config
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .semantic_tokens_enabled
            {
                return Ok(None);
            }
            let uri = params.text_document.uri;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // SLICE the captured stream to the requested lines, then encode:
            // the first kept token's delta is from the document start, which is
            // exactly the encoding a range response specifies. Line granularity
            // is what editors ask with (a viewport), and a token never spans
            // lines (the encoder drops any that would).
            //
            // E122: this used to compute the WHOLE file's stream and filter it
            // — a whole-file walk of the program, plus a raw re-parse, plus one
            // line lookup per token in the file — so twenty visible lines cost
            // what the whole file cost (12.2 ms on kolt's `views.vl`,
            // `proposal/editor-latency.md` §1.6). E121 moved `full` and the
            // delta onto the captured stream but left THIS request on the walk,
            // and the gate below measured it at 0.851× the whole file for a
            // twenty-line window. The slice reads E121's own capture
            // (`LandedSnapshot::tokens`) through the line index built beside it
            // when the analysis landed — one capture, not a second memo of the
            // same tokens.
            //
            // E125: LIVE coordinates, through the same two-sided anchor `full`
            // answers through — because it is answering the same picture, and
            // a viewport that disagreed with the full stream about where a
            // token is is exactly the drift the keystroke path exists to
            // remove. Slicing the capture in the ANALYZED snapshot's
            // coordinates, which is what this did, left every token below an
            // unlanded edit at the line it occupied before the edit until the
            // next analysis landed — on the request an editor sends most.
            let tokens = document.keystroke_tokens_in_lines(
                params.range.start.line,
                params.range.end.line,
                false,
            );
            let data = encode_semantic_tokens(&tokens, &document.line_index);
            Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data,
            })))
        })
    }

    async fn linked_editing_range(
        &self,
        params: LinkedEditingRangeParams,
    ) -> Result<Option<LinkedEditingRanges>> {
        self.fenced("linkedEditingRange", Ok(None), || {
            let uri = params.text_document_position_params.text_document.uri;
            let position = params.text_document_position_params.position;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // Program-space lookup, like hover: the position converts through
            // the ANALYZED index (S1).
            let offset = document.analyzed_offset(position);
            Ok(document
                .linked_tag_ranges(offset)
                .map(|(open, close)| LinkedEditingRanges {
                    ranges: vec![
                        document.analyzed_range(&open),
                        document.analyzed_range(&close),
                    ],
                    word_pattern: None,
                }))
        })
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.fenced("hover", Ok(None), || {
            let uri = params.text_document_position_params.text_document.uri;
            let position = params.text_document_position_params.position;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // Program-space lookup: the position converts through the ANALYZED
            // index, so it names the same character the analysis saw there (S1).
            let offset = document.analyzed_offset(position);
            Ok(document.hover(offset).map(|label| Hover {
                contents: HoverContents::Scalar(MarkedString::String(label)),
                range: None,
            }))
        })
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.fenced("completion", Ok(None), || {
            let uri = params.text_document_position.text_document.uri;
            let position = params.text_document_position.position;
            if is_manifest(&uri) {
                let Some(manifest) = self.manifests.get(&uri) else {
                    return Ok(None);
                };
                let offset = manifest.line_index.offset(position);
                let items = manifest_completion::completions(&manifest.text, offset)
                    .into_iter()
                    .map(|item| to_manifest_item(item, &manifest.line_index))
                    .collect();
                return Ok(Some(CompletionResponse::Array(items)));
            }
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // Deliberately the LIVE index, unlike every sibling handler below:
            // completion's trigger scan (`.`/`?.`/`::`, the partial identifier)
            // reads the buffer the user is mid-keystroke in. `Document::completion`
            // converts to the ANALYZED offset internally wherever it touches
            // `program` data (scope/entity lookups) — see its doc comment (E52).
            let offset = document.line_index.offset(position);
            let mode = self
                .config
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .completion_function_call;
            let snippet_support = self.snippet_support.load(Ordering::Relaxed);
            // E121 §2.1.4. The keystroke path's index answers FIRST, because it
            // is the only source that knows what the live buffer declares: a
            // `fun` typed one keystroke ago is in the index and cannot be in
            // the landed analysis. The landed engine's candidates then fill in
            // everything resolution alone can supply — members, keywords,
            // snippets, auto-imports — and a label the index already offered is
            // dropped rather than repeated. Retiring the engine's own
            // whole-program sweeps behind the index (`auto_import_completions`,
            // `modules_in_root`'s per-request `read_dir`) is the next tranche;
            // this is the seam it happens at.
            let items = document
                .keystroke_completion(offset, false)
                .into_iter()
                .map(|completion| {
                    to_completion_item(completion, mode, snippet_support, &document.line_index)
                })
                .collect();
            Ok(Some(CompletionResponse::Array(items)))
        })
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.fenced("goto_definition", Ok(None), || {
            let uri = params.text_document_position_params.text_document.uri;
            let position = params.text_document_position_params.position;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            let offset = document.analyzed_offset(position);
            let Some((source, span)) = document.definition(offset) else {
                return Ok(None);
            };
            Ok(self
                .location_for(&document, &uri, source, span)
                .map(GotoDefinitionResponse::Scalar))
        })
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        self.fenced("references", Ok(None), || {
            let uri = params.text_document_position.text_document.uri;
            let position = params.text_document_position.position;
            // Every open document at once (kolt.local 034): the union below
            // re-resolves the definition in each neighbor's program, which is
            // what lets a query IN the defining file see the files that import
            // it. One iteration pass collects every guard up front —
            // re-entering the map while holding any of them could deadlock a
            // shard against a concurrent writer.
            let open: Vec<_> = self.documents.iter().collect();
            let Some(origin) = open.iter().find(|entry| *entry.key() == uri) else {
                return Ok(None);
            };
            let offset = origin.value().analyzed_offset(position);
            let neighbors = open
                .iter()
                .filter(|entry| *entry.key() != uri)
                .map(|entry| entry.value());
            let locations = origin
                .value()
                .references_across(offset, neighbors)
                .into_iter()
                .filter_map(|(path, span)| self.location_for_path(&open, &path, span))
                .collect();
            Ok(Some(locations))
        })
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        self.fenced("rename", Err(handler_panicked()), || {
            let uri = params.text_document_position.text_document.uri;
            let position = params.text_document_position.position;
            let new_name = params.new_name;
            // The same one-pass guard collection the references handler uses
            // (kolt.local 034): rename reads the same cross-document union, so
            // a rename issued at a definition rewrites the files that import it.
            let open: Vec<_> = self.documents.iter().collect();
            let Some(origin) = open.iter().find(|entry| *entry.key() == uri) else {
                return Ok(None);
            };
            let document = origin.value();
            // S3: a rename is edits computed from program data. Applying them to a
            // buffer that has moved on corrupts it, so refuse while the snapshots
            // diverge instead of guessing. At human timescales this is invisible —
            // a rename happens at rest, after the debounce has landed. (A STALE
            // NEIGHBOR refuses inside `rename_edits_across`, for the same reason.)
            if document.is_stale() {
                return Err(still_analyzing());
            }
            let offset = document.analyzed_offset(position);
            // Rename is a thin layer over the reference index: the same spans
            // find-references answers with, checked for the extra things a rename
            // needs (a spellable name, files this project may edit, nothing known
            // to be missing) and refused with a reason when any fails.
            let neighbors = open
                .iter()
                .filter(|entry| *entry.key() != uri)
                .map(|entry| entry.value());
            let spans = match document.rename_edits_across(offset, &new_name, neighbors) {
                Ok(spans) => spans,
                Err(crate::document::RenameRefusal::NotAnIdentifier) => return Ok(None),
                Err(refusal) => return Err(rename_refused(&refusal)),
            };
            let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
            for (path, span) in spans {
                // An occurrence that cannot be turned into a location would be a
                // reference this rename silently skips — the partial edit set the
                // rule forbids — so refuse rather than drop it.
                let Some(location) = self.location_for_path(&open, &path, span) else {
                    return Err(rename_refused(
                        &crate::document::RenameRefusal::Incomplete {
                            what: "this symbol".to_string(),
                            missing: 1,
                        },
                    ));
                };
                changes.entry(location.uri).or_default().push(TextEdit {
                    range: location.range,
                    new_text: new_name.clone(),
                });
            }
            Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }))
        })
    }

    /// Whether the cursor is on something renameable, and the range the editor
    /// should pre-fill. Answering this lets the client refuse *before* the user
    /// types a new name, instead of after.
    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        self.fenced("prepare_rename", Ok(None), || {
            let uri = params.text_document.uri;
            let open: Vec<_> = self.documents.iter().collect();
            let Some(origin) = open.iter().find(|entry| *entry.key() == uri) else {
                return Ok(None);
            };
            let document = origin.value();
            if document.is_stale() {
                return Err(still_analyzing());
            }
            let offset = document.analyzed_offset(params.position);
            // `rename_edits_across` with a name known to be valid: the answer
            // is whether a rename COULD proceed, decided by exactly the checks
            // the rename itself will run — neighbors included — so the two
            // cannot disagree.
            let neighbors = open
                .iter()
                .filter(|entry| *entry.key() != uri)
                .map(|entry| entry.value());
            match document.rename_edits_across(offset, "placeholder", neighbors) {
                Ok(_) => {}
                Err(crate::document::RenameRefusal::NotAnIdentifier) => return Ok(None),
                Err(refusal) => return Err(rename_refused(&refusal)),
            }
            let Some(occurrence) = document
                .reference_index()
                .at(SourceId(0), offset)
                .map(|occurrence| occurrence.span)
            else {
                return Ok(None);
            };
            Ok(Some(PrepareRenameResponse::Range(
                document.analyzed_range(&occurrence),
            )))
        })
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        self.fenced("document_symbol", Ok(None), || {
            let uri = params.text_document.uri;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            let symbols = document
                .document_symbols()
                .into_iter()
                .map(|symbol| to_lsp_symbol(symbol, document.analyzed_index()))
                .collect::<Vec<_>>();
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        })
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        self.fenced("formatting", Err(handler_panicked()), || {
            let uri = params.text_document.uri;
            if let Ok(path) = uri.to_file_path()
                && formatting_declined(&path)
            {
                return Ok(None);
            }
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            let source = document.line_index.text();
            let formatted = vilan_core::formatter::format(source);
            // `format` returns the input unchanged when the file is already canonical
            // or hits a construct it can't print (it never produces non-round-tripping
            // output) — either way there is nothing to edit.
            if formatted == source {
                return Ok(None);
            }
            // Replace the whole document in one edit, from the start to the end
            // position the line index reports for the final byte.
            let end = document.line_index.position(source.len());
            Ok(Some(vec![TextEdit {
                range: Range::new(Position::new(0, 0), end),
                new_text: formatted,
            }]))
        })
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.fenced("code_action", Ok(None), || {
            let wants_organize = organize_imports_requested(&params.context.only);
            let wants_quickfix =
                action_kind_requested(&params.context.only, &CodeActionKind::QUICKFIX);
            let wants_fix_all_imports =
                action_kind_requested(&params.context.only, &fix_all_imports_kind());
            let wants_refactor =
                action_kind_requested(&params.context.only, &CodeActionKind::REFACTOR_REWRITE);
            // Skip the work entirely when the client asked for a kind none of
            // these four answer.
            if !wants_organize && !wants_quickfix && !wants_fix_all_imports && !wants_refactor {
                return Ok(None);
            }
            let uri = params.text_document.uri;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // S3, quickfix home (E54/E58): every action below returns edits
            // computed from `program` data — Organize Imports' prune half,
            // the add-import quickfix's candidate scan, the field-rename
            // quickfix's diagnostic note, "add all missing imports". Refuse
            // ALL of them the same way while the snapshots diverge, rather
            // than hand back a half-informed edit set — the SILENT spelling:
            // code actions fire automatically (menu population, the on-save
            // hooks), so this refusal must not toast.
            if document.is_stale() {
                return Err(content_modified());
            }
            let mut actions: Vec<CodeActionOrCommand> = Vec::new();
            if wants_organize {
                let edits = document.organize_import_edits();
                // No edits = already organized (or nothing to do): offer no
                // action, so `codeActionsOnSave` is a clean no-op.
                if !edits.is_empty() {
                    let text_edits: Vec<TextEdit> = edits
                        .into_iter()
                        .map(|(span, new_text)| TextEdit {
                            // Live-space: these spans come from the formatter's own
                            // parse of the live text, not from the program (S2). The
                            // staleness refusal above means the two texts are equal
                            // here anyway.
                            range: document.line_index.range(&span),
                            new_text,
                        })
                        .collect();
                    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                    changes.insert(uri.clone(), text_edits);
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: "Organize Imports".to_string(),
                        kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }
            }
            // css-block S5 (§7.2's refactor): the first NON-diagnostic-driven
            // action in the server, and the only one that needs no `program` —
            // both spellings are read from a raw parse, since neither survives
            // desugaring.
            if wants_refactor
                && let Some(conversion) =
                    document.css_spelling_conversion(live_span(&document, params.range))
            {
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range: document.line_index.range(&conversion.span),
                        new_text: conversion.replacement,
                    }],
                );
                let edit = Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                });
                actions.push(CodeActionOrCommand::CodeAction(if conversion.to_chain {
                    CodeAction {
                        title: "Convert to a `style()` chain".to_string(),
                        kind: Some(CodeActionKind::REFACTOR_REWRITE),
                        edit,
                        ..Default::default()
                    }
                } else {
                    CodeAction {
                        title: "Convert to a `css` block".to_string(),
                        kind: Some(CodeActionKind::REFACTOR_REWRITE),
                        edit,
                        ..Default::default()
                    }
                }));
            }
            if let Some(program) = document.program.as_ref() {
                if wants_quickfix {
                    let range = live_span(&document, params.range);
                    for fix in document.quickfixes(program, range) {
                        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                        changes.insert(
                            uri.clone(),
                            vec![TextEdit {
                                range: document.line_index.range(&fix.span),
                                new_text: fix.replacement,
                            }],
                        );
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: fix.title,
                            kind: Some(CodeActionKind::QUICKFIX),
                            edit: Some(WorkspaceEdit {
                                changes: Some(changes),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }
                }
                if wants_fix_all_imports
                    && let Some((span, new_text)) = document.add_all_missing_imports_edit(program)
                {
                    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range: document.line_index.range(&span),
                            new_text,
                        }],
                    );
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: "Add All Missing Imports".to_string(),
                        kind: Some(fix_all_imports_kind()),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }
            }
            if actions.is_empty() {
                Ok(None)
            } else {
                Ok(Some(actions))
            }
        })
    }
}

/// The LIVE-space `Span` a code-action request's `range` covers — the same
/// conversion `Document::completion`'s trigger scan uses, just for a `Range`
/// instead of one `Position`.
fn live_span(document: &Document, range: Range) -> Span {
    Span {
        start: document.line_index.offset(range.start),
        end: document.line_index.offset(range.end),
    }
}

/// The "add all missing imports" source action's kind (E54d): a `source.fixAll`
/// sub-kind, following the convention `source.organizeImports` already sets
/// (a specific source action names itself under the general one it refines) —
/// there is no `executeCommand` infrastructure at all, and a source-action
/// kind is what avoids ever needing one.
fn fix_all_imports_kind() -> CodeActionKind {
    CodeActionKind::new("source.fixAll.imports")
}

/// Whether a code-action request wants `kind` — an unfiltered request (no
/// `only`) always does, and a filtered one does when it lists `kind` itself
/// or an ANCESTOR of it, per the LSP `.`-delimited kind hierarchy (a
/// requested `source` matches `source.organizeImports`; a requested
/// `quickfix` matches a hypothetical `quickfix.something`).
fn action_kind_requested(only: &Option<Vec<CodeActionKind>>, kind: &CodeActionKind) -> bool {
    let Some(kinds) = only else {
        return true;
    };
    kinds.iter().any(|requested| {
        kind == requested
            || kind
                .as_str()
                .strip_prefix(requested.as_str())
                .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// Whether a code-action request wants the Organize Imports source action:
/// `source.organizeImports` or an ancestor kind (`source`).
fn organize_imports_requested(only: &Option<Vec<CodeActionKind>>) -> bool {
    action_kind_requested(only, &CodeActionKind::SOURCE_ORGANIZE_IMPORTS)
}

/// B40: request handlers are panic-fenced. A panic used to unwind through
/// the async runtime and abort the whole server — exit 101, five crashes in
/// three minutes and the client stops restarting it. These prove the fence
/// (a panicked handler answers its fallback) and, as important, that the
/// server KEEPS answering afterwards — a poisoned lock would turn the first
/// caught panic into a panic on every later request.
#[cfg(test)]
mod panic_fence_tests {
    use super::snapshot_consistency_tests::{SOURCE, backend, open_with_live_edit, rename_params};
    use super::*;

    thread_local! {
        /// The handler whose next fenced body panics (consumed on fire).
        /// Thread-local: `#[tokio::test]`'s current-thread runtime polls the
        /// handler future on the test's own thread, so an armed injection
        /// cannot leak into a concurrently running test's handler calls.
        static INJECT: std::cell::Cell<Option<&'static str>> =
            const { std::cell::Cell::new(None) };
    }

    /// Called by `Backend::fenced` at the top of every fenced body.
    pub(crate) fn maybe_inject(request: &'static str) {
        INJECT.with(|slot| {
            if slot.get() == Some(request) {
                slot.set(None);
                panic!("test-injected {request} panic");
            }
        });
    }

    fn arm(request: &'static str) {
        INJECT.with(|slot| slot.set(Some(request)));
    }

    fn hover_params(uri: &Url, position: Position) -> HoverParams {
        HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
        }
    }

    // Read-only queries degrade to their empty answer; the next request runs
    // the normal path — the server, its document map, and its locks all
    // survive the caught panic.
    #[tokio::test]
    async fn a_panicked_query_answers_empty_and_the_server_keeps_serving() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, SOURCE);
        // Line 1, inside `value` — a position that hovers in the normal path.
        let position = Position::new(1, 6);
        arm("hover");
        let fenced = backend.hover(hover_params(&uri, position)).await;
        assert_eq!(fenced.expect("the fallback, not an error"), None);
        let after = backend.hover(hover_params(&uri, position)).await;
        assert!(
            after.expect("a normal answer").is_some(),
            "the request after a caught panic must run the normal path"
        );
    }

    // Edit-producing requests refuse instead: a rename answering `None`
    // would read as "nothing to rename", which a failure is not.
    #[tokio::test]
    async fn a_panicked_rename_refuses_and_the_server_keeps_serving() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, SOURCE);
        let position = Position::new(1, 6);
        arm("rename");
        let refused = backend
            .rename(rename_params(&uri, position))
            .await
            .expect_err("a panicked rename must refuse, not answer");
        assert_eq!(refused.code, ErrorCode::ServerError(-32803));
        assert!(refused.message.contains("vilan-lsp bug"), "{refused:?}");
        let after = backend.rename(rename_params(&uri, position)).await;
        assert!(
            after.expect("a normal answer").is_some(),
            "the rename after a caught panic must run the normal path"
        );
    }
}

#[cfg(test)]
mod snapshot_consistency_tests {
    use super::*;
    use crate::document::tests::std_root;
    use tower_lsp::ClientSocket;

    pub(crate) const SOURCE: &str = "fun main() {\n\tlet value = 1;\n\tlet other = value;\n}\n";
    /// The same program with one character inserted on line 0, so every later
    /// byte shifts: what the buffer looks like mid-keystroke.
    const EDITED: &str = "fun  main() {\n\tlet value = 1;\n\tlet other = value;\n}\n";

    fn document(text: &str) -> Document {
        Document::analyze(text, &std_root(), Path::new("snapshot.vl"))
    }

    fn uri() -> Url {
        Url::parse("file:///snapshot/main.vl").expect("a url")
    }

    /// A real `Backend` (the socket is returned so the client half stays
    /// alive). Only handlers that never talk to the client are driven through
    /// it — the ones this file guards.
    pub(crate) fn backend() -> (LspService<Backend>, ClientSocket) {
        LspService::new(|client| Backend {
            client,
            documents: Arc::new(DashMap::new()),
            semantic_token_cache: Arc::new(DashMap::new()),
            manifests: Arc::new(DashMap::new()),
            publish_state: Arc::new(std::sync::Mutex::new(PublishState::new())),
            pending: Arc::new(DashMap::new()),
            line_indices: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(Config::default())),
            snippet_support: Arc::new(AtomicBool::new(false)),
            revision: Arc::new(AtomicU64::new(0)),
            publish_gate: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    /// An open document at [`uri`], analyzed from `SOURCE`, with `live` applied
    /// as an un-analyzed edit.
    pub(crate) fn open_with_live_edit(backend: &Backend, live: &str) -> Url {
        let uri = uri();
        backend.documents.insert(uri.clone(), document(SOURCE));
        backend
            .documents
            .get_mut(&uri)
            .expect("just inserted")
            .set_text(live);
        uri
    }

    pub(crate) fn rename_params(uri: &Url, position: Position) -> RenameParams {
        RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            new_name: "renamed".to_string(),
            work_done_progress_params: Default::default(),
        }
    }

    fn code_action_params(uri: &Url) -> CodeActionParams {
        CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range::default(),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    // S3, handler 1 of 2: rename returns text edits computed from program
    // spans. Applying them to a buffer that has moved on lands them at the
    // wrong offsets — the one failure mode that corrupts a file rather than
    // merely looking wrong — so it refuses while the snapshots diverge.
    #[tokio::test]
    async fn rename_refuses_while_the_buffer_is_ahead_of_the_analysis() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, EDITED);
        let position = Position::new(1, 6); // inside `value`
        let error = backend
            .rename(rename_params(&uri, position))
            .await
            .expect_err("a stale rename refuses");
        assert_eq!(error.code, ErrorCode::ServerError(-32803));
        assert!(
            error.message.contains("still analyzing"),
            "{}",
            error.message
        );
    }

    // …and it answers normally once the buffer and the analysis agree again —
    // the non-vacuity half, so the refusal can't be a permanent refusal.
    #[tokio::test]
    async fn rename_answers_once_the_snapshots_agree() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, SOURCE);
        let edit = backend
            .rename(rename_params(&uri, Position::new(1, 6)))
            .await
            .expect("a fresh rename answers")
            .expect("`value` is renameable");
        let changes = edit.changes.expect("one file's edits");
        assert_eq!(
            changes[&uri].len(),
            2,
            "the declaration and its use are renamed",
        );
    }

    // S3, handler 2 of 2: Organize Imports also returns text edits, and its
    // prune half reads program data. Its refusal is the SILENT spelling —
    // `ContentModified`, which `vscode-languageclient` swallows into the
    // default empty answer — because code actions fire automatically (menu
    // population, the on-save hooks): `RequestFailed` here would pop an error
    // toast on every save inside the debounce window.
    #[tokio::test]
    async fn organize_imports_refuses_while_the_buffer_is_ahead_of_the_analysis() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, EDITED);
        let error = backend
            .code_action(code_action_params(&uri))
            .await
            .expect_err("a stale organize refuses");
        assert_eq!(error.code, ErrorCode::ContentModified);
        assert!(
            error.message.contains("still analyzing"),
            "{}",
            error.message
        );
    }

    // A code-action request for a kind we don't offer AT ALL is answered
    // before the staleness gate: refusing there would make every unrelated
    // request fail mid-typing. (`refactor.extract` rather than the bare
    // `refactor` this used to name: css-block S5 registered
    // `refactor.rewrite`, and the bare kind is its ANCESTOR — so it is a kind
    // the server now partly answers, and it belongs with the group below.)
    #[tokio::test]
    async fn a_stale_document_still_answers_an_unoffered_code_action_kind() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, EDITED);
        let mut params = code_action_params(&uri);
        params.context.only = Some(vec![CodeActionKind::REFACTOR_EXTRACT]);
        assert!(
            backend
                .code_action(params)
                .await
                .expect("not a refusal")
                .is_none(),
        );
    }

    // E54/E58 (quickfix home, part a): QUICKFIX and "add all missing
    // imports" are OFFERED kinds now, so — unlike a kind we never answer at
    // all — they refuse the SAME way Organize Imports does while the buffer
    // is ahead of the analysis: their edits are computed from `program` data
    // too (the diagnostic scan, the candidate search), so a half-informed
    // edit set is exactly as unsafe here.
    #[tokio::test]
    async fn a_stale_document_refuses_a_quickfix_request() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, EDITED);
        let mut params = code_action_params(&uri);
        params.context.only = Some(vec![CodeActionKind::QUICKFIX]);
        let error = backend
            .code_action(params)
            .await
            .expect_err("a stale quickfix request refuses");
        assert_eq!(error.code, ErrorCode::ContentModified);
    }

    // css-block S5: `refactor.rewrite` is an OFFERED kind, so it joins the
    // group above — and so does the bare `refactor` an editor's refactor menu
    // asks with, since the kind hierarchy makes it an ancestor. The conversion
    // itself reads a raw parse rather than `program`, but the handler's
    // refusal is one answer for the whole request, and half-answering a menu
    // is not better than refusing it.
    #[tokio::test]
    async fn a_stale_document_refuses_a_refactor_request() {
        for kind in [CodeActionKind::REFACTOR_REWRITE, CodeActionKind::REFACTOR] {
            let (service, _socket) = backend();
            let backend = service.inner();
            let uri = open_with_live_edit(backend, EDITED);
            let mut params = code_action_params(&uri);
            params.context.only = Some(vec![kind.clone()]);
            let error = backend
                .code_action(params)
                .await
                .expect_err("a stale refactor request refuses");
            assert_eq!(error.code, ErrorCode::ContentModified, "{kind:?}");
        }
    }

    #[tokio::test]
    async fn a_stale_document_refuses_an_add_all_missing_imports_request() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, EDITED);
        let mut params = code_action_params(&uri);
        params.context.only = Some(vec![super::fix_all_imports_kind()]);
        let error = backend
            .code_action(params)
            .await
            .expect_err("a stale fix-all request refuses");
        assert_eq!(error.code, ErrorCode::ContentModified);
    }

    /// Inserts an already-analyzed multi-file `Document` (built with real
    /// `pkg` siblings on disk, via `document::tests::analyze_workspace`) and
    /// returns its uri plus the full-file `Range` — the coordinates a
    /// `CodeActionParams.range` needs to overlap every diagnostic in it.
    fn open_analyzed(backend: &Backend, document: Document) -> (Url, Range) {
        let uri = Url::parse("file:///quickfix-home/main.vl").expect("a url");
        let range = Range::new(
            document.line_index.position(0),
            document
                .line_index
                .position(document.line_index.text().len()),
        );
        backend.documents.insert(uri.clone(), document);
        (uri, range)
    }

    // E54(a)/(b), end to end: the QUICKFIX kind is registered, routed, and
    // answers a real unresolved-name diagnostic with the add-import fix —
    // through `Backend::code_action` itself, not just the data layer.
    #[tokio::test]
    async fn quickfix_add_import_is_offered_and_carries_its_edit_through_the_real_handler() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (_dir, document) = crate::document::tests::analyze_workspace(&[
            ("main.vl", "fun main() {\n\thelp_topic();\n}\n"),
            ("topic.vl", "fun help_topic() {}\n"),
        ]);
        let (uri, range) = open_analyzed(backend, document);
        let mut params = code_action_params(&uri);
        params.range = range;
        params.context.only = Some(vec![CodeActionKind::QUICKFIX]);
        let response = backend
            .code_action(params)
            .await
            .expect("not stale")
            .expect("a quickfix is offered");
        assert_eq!(response.len(), 1, "{response:#?}");
        let CodeActionOrCommand::CodeAction(action) = &response[0] else {
            panic!("expected a CodeAction, got {:?}", response[0]);
        };
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert!(action.title.contains("help_topic"), "{}", action.title);
        assert!(action.title.contains("pkg::topic"), "{}", action.title);
        let edits = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .and_then(|changes| changes.get(&uri))
            .expect("an edit for this file");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "import pkg::topic::help_topic;\n");
    }

    // css-block S5, end to end: the server's first `refactor.rewrite` action
    // is registered, routed, and carries its edit — through
    // `Backend::code_action` itself, and driven by where the CURSOR is rather
    // than by a diagnostic, which is what makes it a new code path rather than
    // a fifth quickfix.
    #[tokio::test]
    async fn the_css_spelling_refactor_is_offered_through_the_real_handler() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let source = "import std::style::{ Style, style };\n\nfun card(): Style {\n\tcss {\n\t\tdisplay: flex;\n\t}\n}\n";
        let (_dir, document) = crate::document::tests::analyze_workspace(&[("main.vl", source)]);
        let cursor = document
            .line_index
            .position(source.find("display").expect("the fixture"));
        let (uri, _range) = open_analyzed(backend, document);
        let mut params = code_action_params(&uri);
        params.range = Range::new(cursor, cursor);
        params.context.only = Some(vec![CodeActionKind::REFACTOR_REWRITE]);
        let response = backend
            .code_action(params)
            .await
            .expect("not stale")
            .expect("a refactor is offered");
        let CodeActionOrCommand::CodeAction(action) = &response[0] else {
            panic!("expected a CodeAction, got {:?}", response[0]);
        };
        assert_eq!(action.kind, Some(CodeActionKind::REFACTOR_REWRITE));
        assert_eq!(action.title, "Convert to a `style()` chain");
        let edits = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .and_then(|changes| changes.get(&uri))
            .expect("an edit for this file");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "style().raw(\"display\", \"flex\")");
    }

    // E54(d), end to end: "Add All Missing Imports" is registered under its
    // own `source.fixAll.imports` kind and returns one edit fixing every
    // unambiguous name.
    #[tokio::test]
    async fn add_all_missing_imports_is_offered_through_the_real_handler() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (_dir, document) = crate::document::tests::analyze_workspace(&[
            ("main.vl", "fun main() {\n\thelp_topic();\n}\n"),
            ("topic.vl", "fun help_topic() {}\n"),
        ]);
        let (uri, range) = open_analyzed(backend, document);
        let mut params = code_action_params(&uri);
        params.range = range;
        params.context.only = Some(vec![super::fix_all_imports_kind()]);
        let response = backend
            .code_action(params)
            .await
            .expect("not stale")
            .expect("an action is offered");
        assert_eq!(response.len(), 1, "{response:#?}");
        let CodeActionOrCommand::CodeAction(action) = &response[0] else {
            panic!("expected a CodeAction, got {:?}", response[0]);
        };
        assert_eq!(action.kind, Some(super::fix_all_imports_kind()));
        let edits = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .and_then(|changes| changes.get(&uri))
            .expect("an edit for this file");
        assert_eq!(edits.len(), 1);
        assert!(
            edits[0].new_text.contains("import pkg::topic::help_topic;"),
            "{}",
            edits[0].new_text
        );
    }

    // E58(c), end to end: a misspelled initializer field's closest-name note
    // becomes a QUICKFIX that rewrites exactly the field-name span.
    #[tokio::test]
    async fn quickfix_rewrites_a_misspelled_field_through_the_real_handler() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (_dir, document) = crate::document::tests::analyze_workspace(&[(
            "main.vl",
            "struct Config {\n\tentries: i32,\n}\n\nfun main() {\n\tlet _ = Config { entires = 5 };\n}\n",
        )]);
        let (uri, range) = open_analyzed(backend, document);
        let mut params = code_action_params(&uri);
        params.range = range;
        params.context.only = Some(vec![CodeActionKind::QUICKFIX]);
        let response = backend
            .code_action(params)
            .await
            .expect("not stale")
            .expect("a quickfix is offered");
        assert_eq!(response.len(), 1, "{response:#?}");
        let CodeActionOrCommand::CodeAction(action) = &response[0] else {
            panic!("expected a CodeAction, got {:?}", response[0]);
        };
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.title, "Change to `entries`");
        let edits = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .and_then(|changes| changes.get(&uri))
            .expect("an edit for this file");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "entries");
    }

    // E61/S2, end to end (editing-dx.md §17.4): the parser's own gap-anchored
    // "expected `;` to end this statement" diagnostic becomes a QUICKFIX that
    // inserts exactly `;` at the gap — a zero-width edit right after the
    // token the `;` was missing from.
    #[tokio::test]
    async fn quickfix_inserts_a_missing_semicolon_through_the_real_handler() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (_dir, document) = crate::document::tests::analyze_workspace(&[(
            "main.vl",
            "fun main() {\n\tlet x: i32 = 1\n\tx;\n}\n",
        )]);
        let (uri, range) = open_analyzed(backend, document);
        let mut params = code_action_params(&uri);
        params.range = range;
        params.context.only = Some(vec![CodeActionKind::QUICKFIX]);
        let response = backend
            .code_action(params)
            .await
            .expect("not stale")
            .expect("a quickfix is offered");
        assert_eq!(response.len(), 1, "{response:#?}");
        let CodeActionOrCommand::CodeAction(action) = &response[0] else {
            panic!("expected a CodeAction, got {:?}", response[0]);
        };
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.title, "Insert `;`");
        let edits = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .and_then(|changes| changes.get(&uri))
            .expect("an edit for this file");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, ";");
        assert_eq!(
            edits[0].range.start, edits[0].range.end,
            "a zero-width insertion, not a replacement"
        );
        // Right after the `1` that ends `let x: i32 = 1` (line 1, 0-based) —
        // the gap `gap_span` anchors, not the head of the next statement.
        assert_eq!(edits[0].range.start, Position::new(1, 15));
    }

    // E61/S3-residual, end to end (editing-dx.md §17.4): regime 1's `;`
    // discards a value" diagnostic — anchored at the callable's closing
    // BRACE, not the `;` itself — becomes a QUICKFIX that removes exactly
    // the `;` it names, located from the program's own last-statement
    // bookkeeping rather than the diagnostic's own span.
    #[tokio::test]
    async fn quickfix_removes_a_discarding_semicolon_through_the_real_handler() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (_dir, document) = crate::document::tests::analyze_workspace(&[(
            "main.vl",
            "fun total(a: i32, b: i32): i32 {\n\ta + b;\n}\n\nfun main() {\n\ttotal(1, 2);\n}\n",
        )]);
        let (uri, range) = open_analyzed(backend, document);
        let mut params = code_action_params(&uri);
        params.range = range;
        params.context.only = Some(vec![CodeActionKind::QUICKFIX]);
        let response = backend
            .code_action(params)
            .await
            .expect("not stale")
            .expect("a quickfix is offered");
        assert_eq!(response.len(), 1, "{response:#?}");
        let CodeActionOrCommand::CodeAction(action) = &response[0] else {
            panic!("expected a CodeAction, got {:?}", response[0]);
        };
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.title, "Remove `;`");
        let edits = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .and_then(|changes| changes.get(&uri))
            .expect("an edit for this file");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "");
        // The `;` right after `a + b` (line 1, 0-based) — not the closing
        // brace the diagnostic itself anchors at.
        assert_eq!(edits[0].range.start, Position::new(1, 6));
        assert_eq!(edits[0].range.end, Position::new(1, 7));
    }

    // The `;`-locating scan's own edge case: something OTHER than whitespace
    // between the last statement and its `;` (a `//` comment, here) makes it
    // DECLINE rather than guess past it — the diagnostic still fires
    // (unaffected — this is a fix-offering question, not a diagnostic one),
    // but no quickfix is offered for it (B4: no fix beats a wrong one).
    #[tokio::test]
    async fn no_remove_semicolon_quickfix_is_offered_when_a_comment_sits_in_the_gap() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (_dir, document) = crate::document::tests::analyze_workspace(&[(
            "main.vl",
            "fun total(a: i32, b: i32): i32 {\n\ta + b // trailing comment\n\t;\n}\n\nfun main() {\n\ttotal(1, 2);\n}\n",
        )]);
        let (uri, range) = open_analyzed(backend, document);
        let mut params = code_action_params(&uri);
        params.range = range;
        params.context.only = Some(vec![CodeActionKind::QUICKFIX]);
        let response = backend.code_action(params).await.expect("not stale");
        assert!(
            response.is_none(),
            "expected no quickfix offered when a comment sits in the gap; got {response:#?}"
        );
    }

    // S1/S3: read-only queries never refuse — they answer
    // correctly-for-the-snapshot.
    //
    // **E121 (RULED 2026-09-01) narrows what "the snapshot" means for this one
    // handler, and this pin is rewritten to the narrower rule.** S1's original
    // claim was that the whole token stream comes back byte-identical to the
    // pre-edit answer — the highlighting holds still for the full staleness
    // window, measured at 409 ms on the fast file and 1.1 s on the slow one
    // (`proposal/editor-latency.md` §1.5). Q5 rules that out: *"commenting a
    // line out must read as a comment at once, not keep its semantic colors for
    // the staleness window"*. The keystroke path therefore repaints the EDIT
    // WINDOW from syntax on every keystroke and re-maps everything outside it
    // through the two-sided anchor.
    //
    // So the property this pin now holds is the sharper, true one: **the
    // anchors hold still and the window tracks the buffer.** Nothing is lost,
    // no classification changes, and the only movement is the edited line's own
    // token following the character that was typed in front of it.
    #[tokio::test]
    async fn semantic_tokens_track_the_edit_window_and_anchor_the_rest() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend.documents.insert(uri.clone(), document(SOURCE));
        let params = SemanticTokensParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let baseline = backend
            .semantic_tokens_full(params.clone())
            .await
            .expect("tokens");
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(EDITED);
        let mid_edit = backend
            .semantic_tokens_full(params)
            .await
            .expect("tokens while typing");
        // The `result_id` is fresh per response by design (B39b's delta chain),
        // so the comparison names the data.
        let data_of = |answer: Option<SemanticTokensResult>| match answer {
            Some(SemanticTokensResult::Tokens(tokens)) => tokens.data,
            other => panic!("the full provider returns tokens, got {other:?}"),
        };
        let baseline = data_of(baseline);
        let mid_edit = data_of(mid_edit);
        assert!(!baseline.is_empty(), "the fixture must produce tokens");
        assert_eq!(
            baseline.len(),
            mid_edit.len(),
            "no token may be lost mid-keystroke: the window repaints from syntax and the \
             anchors re-map, so the stream keeps its shape",
        );
        // `EDITED` inserts one space on line 0, so `main` — the only token in
        // the edit window — starts one column later, and its CLASS is
        // unchanged because syntax alone decides that an identifier after `fun`
        // is a function declaration.
        assert_eq!(
            (
                mid_edit[0].delta_line,
                mid_edit[0].delta_start,
                mid_edit[0].token_type,
                mid_edit[0].token_modifiers_bitset,
            ),
            (
                baseline[0].delta_line,
                baseline[0].delta_start + 1,
                baseline[0].token_type,
                baseline[0].token_modifiers_bitset,
            ),
            "the token in the edit window must follow the character typed in front of it",
        );
        // Everything below the edited line rode the anchor: the encoding is
        // relative, and byte-identical text at a constant shift encodes
        // identically.
        assert_eq!(
            &baseline[1..],
            &mid_edit[1..],
            "every token outside the edit window sits on byte-identical text, so its answer \
             is exact and unmoved",
        );
    }

    // S1: inlay hints, same property — and the viewport filter is what made
    // them vanish rather than merely shift, so the request asks for the whole
    // file's range exactly as the editor does.
    #[tokio::test]
    async fn inlay_hints_answer_the_analyzed_snapshot_while_typing() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend.documents.insert(uri.clone(), document(SOURCE));
        let params = InlayHintParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range::new(Position::new(0, 0), Position::new(100, 0)),
            work_done_progress_params: Default::default(),
        };
        let baseline = backend.inlay_hint(params.clone()).await.expect("hints");
        assert!(
            baseline.as_ref().is_some_and(|hints| !hints.is_empty()),
            "the fixture must produce hints",
        );
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(EDITED);
        let mid_edit = backend
            .inlay_hint(params)
            .await
            .expect("hints while typing");
        assert_eq!(format!("{baseline:?}"), format!("{mid_edit:?}"));
    }

    // --- Handler wiring: the S1 index switch, pinned per handler ------------
    //
    // Case 9 pins the inbound mechanism on `Document`; these pin that each
    // HANDLER actually routes through it — reverting any one handler line back
    // to the live index fails its pin. The fixture prepends a line, which
    // changes the LINE every analyzed byte offset converts to through the live
    // index; and the two locals carry different types (`i32` vs `str`), so a
    // wrongly-wired inbound lookup that slides from `other` onto `value`
    // cannot answer an accidentally-identical hover.

    /// `WIRING_SOURCE` with a comment line prepended: every analyzed offset's
    /// live-index conversion moves down a line (or, near the top, onto the
    /// comment's bytes).
    const WIRING_SOURCE: &str = "fun main() {\n\tlet value = 1;\n\tlet other = \"text\";\n}\n";
    const WIRING_EDITED_HEAD: &str = "// a new first line\n";

    /// Apply the prepend to the open document at `uri` as an un-analyzed edit —
    /// with the guard that the edit really skews the inbound conversion at
    /// `position` (otherwise the equality assertions above it prove nothing).
    fn apply_wiring_edit(backend: &Backend, uri: &Url, position: Position) {
        let mut edited = String::from(WIRING_EDITED_HEAD);
        edited.push_str(WIRING_SOURCE);
        backend
            .documents
            .get_mut(uri)
            .expect("open")
            .set_text(&edited);
        let document = backend.documents.get(uri).expect("open");
        assert_ne!(
            document.analyzed_offset(position),
            document.line_index.offset(position),
            "the fixture must skew the inbound conversion at {position:?}",
        );
    }

    /// The `other` declaration in [`WIRING_SOURCE`]: line 2, inside the name.
    fn other_decl() -> Position {
        Position::new(2, 6)
    }

    /// The `Position` of `needle` in `text`, plus `delta` characters — the
    /// `Position` analogue of `document::tests::offset_at`, for the completion
    /// pins below, which drive the `Backend` (positions, not raw offsets).
    /// ASCII-only fixtures throughout, so a byte offset doubles as a UTF-16
    /// `character` count.
    fn position_at(text: &str, needle: &str, delta: usize) -> Position {
        let offset = text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} not found in the pin source"))
            + delta;
        LineIndex::new(text).position(offset)
    }

    #[tokio::test]
    async fn hover_answers_the_analyzed_snapshot_while_typing() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend
            .documents
            .insert(uri.clone(), document(WIRING_SOURCE));
        let params = |uri: &Url| HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: other_decl(),
            },
            work_done_progress_params: Default::default(),
        };
        let baseline = backend.hover(params(&uri)).await.expect("hover");
        assert!(baseline.is_some(), "the fixture must hover");
        apply_wiring_edit(backend, &uri, other_decl());
        let mid_edit = backend.hover(params(&uri)).await.expect("hover mid-edit");
        assert_eq!(format!("{baseline:?}"), format!("{mid_edit:?}"));
    }

    // The member path (E72): a FIELD hover through the handler answers the
    // house-styled `name: T`, and keeps answering the analyzed snapshot while
    // an un-analyzed edit is pending — the same wiring pin as above, on the
    // member fallback the format change routed differently.
    #[tokio::test]
    async fn member_hover_answers_the_house_style_while_typing() {
        const MEMBER_SOURCE: &str = "struct Point {\n\tx: i32,\n}\n\nfun main() {\n\tlet p = Point { x = 1 };\n\tlet n = p.x;\n}\n";
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend
            .documents
            .insert(uri.clone(), document(MEMBER_SOURCE));
        let field = position_at(MEMBER_SOURCE, "p.x", 2);
        let params = |uri: &Url| HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: field,
            },
            work_done_progress_params: Default::default(),
        };
        let baseline = backend.hover(params(&uri)).await.expect("hover");
        let rendered = format!("{baseline:?}");
        assert!(
            rendered.contains("x: i32"),
            "the field hovers in the house shape: {rendered}"
        );
        // The un-analyzed edit: a prepended comment line skews every
        // conversion below it.
        let mut edited = String::from("// a new first line\n");
        edited.push_str(MEMBER_SOURCE);
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&edited);
        {
            let document = backend.documents.get(&uri).expect("open");
            assert_ne!(
                document.analyzed_offset(field),
                document.line_index.offset(field),
                "the fixture must skew the inbound conversion",
            );
        }
        let mid_edit = backend.hover(params(&uri)).await.expect("hover mid-edit");
        assert_eq!(rendered, format!("{mid_edit:?}"));
    }

    #[tokio::test]
    async fn goto_definition_answers_the_analyzed_snapshot_while_typing() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend
            .documents
            .insert(uri.clone(), document(WIRING_SOURCE));
        let params = |uri: &Url| GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: other_decl(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let baseline = backend.goto_definition(params(&uri)).await.expect("def");
        assert!(baseline.is_some(), "the fixture must resolve a definition");
        apply_wiring_edit(backend, &uri, other_decl());
        let mid_edit = backend
            .goto_definition(params(&uri))
            .await
            .expect("def mid-edit");
        assert_eq!(format!("{baseline:?}"), format!("{mid_edit:?}"));
    }

    #[tokio::test]
    async fn references_answer_the_analyzed_snapshot_while_typing() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend
            .documents
            .insert(uri.clone(), document(WIRING_SOURCE));
        let params = |uri: &Url| ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: other_decl(),
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let baseline = backend.references(params(&uri)).await.expect("refs");
        assert!(
            baseline.as_ref().is_some_and(|refs| !refs.is_empty()),
            "the fixture must find references",
        );
        apply_wiring_edit(backend, &uri, other_decl());
        let mid_edit = backend
            .references(params(&uri))
            .await
            .expect("refs mid-edit");
        assert_eq!(format!("{baseline:?}"), format!("{mid_edit:?}"));
    }

    #[tokio::test]
    async fn document_symbols_answer_the_analyzed_snapshot_while_typing() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend
            .documents
            .insert(uri.clone(), document(WIRING_SOURCE));
        let params = |uri: &Url| DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let baseline = backend
            .document_symbol(params(&uri))
            .await
            .expect("symbols");
        assert!(baseline.is_some(), "the fixture must outline");
        apply_wiring_edit(backend, &uri, other_decl());
        // The outbound guard: the outline's spans really do convert differently
        // through the two indices after the edit (the prepend moves the
        // function's END line, so the full range differs even though its start
        // does not).
        {
            let document = backend.documents.get(&uri).expect("open");
            let full = document.document_symbols()[0].full;
            assert_ne!(
                document.analyzed_range(&full),
                document.line_index.range(&full),
                "the fixture must skew the outbound conversion",
            );
        }
        let mid_edit = backend
            .document_symbol(params(&uri))
            .await
            .expect("symbols mid-edit");
        assert_eq!(format!("{baseline:?}"), format!("{mid_edit:?}"));
    }

    // E52: completion was the one query left wired to the LIVE index for its
    // scope/entity lookups — every other handler above converts through
    // `analyzed_offset` first. A scope-position completion (no `.`/`::` before
    // the cursor) must resolve the SAME enclosing scope while the buffer is
    // ahead of the analysis.
    //
    // TWO functions, deliberately, rather than `WIRING_SOURCE`'s one: `value`
    // and `other` there share a single scope, so ANY offset near that fixture
    // resolves the same scope-completion set regardless of which byte it names
    // — a pin built on it would stay green even feeding the raw live offset
    // straight into `scope_at` (proven while designing this pin: it did). Two
    // functions means the wrong scope is a different, checkable set of names.
    #[tokio::test]
    async fn completion_answers_the_analyzed_snapshot_while_typing() {
        const SCOPE_SOURCE: &str =
            "fun first() {\n\tlet alpha = 1;\n}\n\nfun second() {\n\tlet beta = 2;\n}\n";
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend
            .documents
            .insert(uri.clone(), document(SCOPE_SOURCE));
        // Inside `beta`'s declaration — a scope position, no `.`/`::` before it.
        let position = position_at(SCOPE_SOURCE, "beta", 2);
        let params = |uri: &Url| CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };
        let baseline = backend.completion(params(&uri)).await.expect("completion");
        let labels_of = |response: &Option<CompletionResponse>| match response {
            Some(CompletionResponse::Array(items)) => items
                .iter()
                .map(|item| item.label.clone())
                .collect::<Vec<_>>(),
            other => panic!("the array form is expected, got {other:?}"),
        };
        assert!(
            labels_of(&baseline).contains(&"beta".to_string()),
            "the fixture must offer the local: {baseline:?}",
        );
        // Widen the FIRST line by enough bytes that the live offset this
        // `Position` names overruns the whole analyzed text — line/column are
        // unchanged, so the trigger scan still takes the scope-completion
        // branch both times, isolating the divergence to `scope_at`.
        let edited = SCOPE_SOURCE.replacen("fun first", &format!("fun {}first", "x".repeat(80)), 1);
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&edited);
        {
            let document = backend.documents.get(&uri).expect("open");
            assert_ne!(
                document.analyzed_offset(position),
                document.line_index.offset(position),
                "the fixture must skew the inbound conversion at {position:?}",
            );
        }
        let mid_edit = backend
            .completion(params(&uri))
            .await
            .expect("completion mid-edit");
        assert_eq!(
            labels_of(&baseline),
            labels_of(&mid_edit),
            "the same enclosing scope resolves from the analyzed snapshot",
        );
    }

    // E52, member-completion variant: the RECEIVER's type also resolves
    // through a `program` lookup (`entity_at`, off a complex/chained receiver
    // rather than a bare name — `widget` is a FIELD, not a binding, so
    // `binding_in_scope`/`same_file_variable` cannot resolve it and completion
    // falls all the way to the `entity_at`-only path, which has no
    // scope-search fallback to mask an offset error), and must answer the
    // analyzed snapshot too — the "pre-edit receiver" symptom.
    //
    // Unlike the WIRING fixtures above, this skew widens an EARLY LINE by a
    // few characters rather than prepending a whole one: the receiver's own
    // line/column never move, only its BYTE offset does, so the SAME
    // `Position` is valid before and after — completion's live trigger scan
    // (deliberately untouched by the fix) still finds the same `.` both
    // times, isolating the divergence to the downstream entity lookup.
    #[tokio::test]
    async fn member_completion_answers_the_analyzed_snapshot_while_typing() {
        const MEMBER_SOURCE: &str = "struct Widget {\n\tsize: i32,\n}\n\nstruct Container {\n\twidget: Widget,\n}\n\nfun main() {\n\tlet box = Container { widget = Widget { size = 1 } };\n\tbox.widget.size;\n}\n";
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend
            .documents
            .insert(uri.clone(), document(MEMBER_SOURCE));
        // Right after `box.widget.`, before `size` — a member trigger on a
        // chained field access, with no typed prefix.
        let position = position_at(MEMBER_SOURCE, "box.widget.size", 11);
        let params = |uri: &Url| CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };
        let baseline = backend.completion(params(&uri)).await.expect("completion");
        let labels_of = |response: &Option<CompletionResponse>| match response {
            Some(CompletionResponse::Array(items)) => items
                .iter()
                .map(|item| item.label.clone())
                .collect::<Vec<_>>(),
            other => panic!("the array form is expected, got {other:?}"),
        };
        assert!(
            labels_of(&baseline).contains(&"size".to_string()),
            "the fixture must offer the field: {baseline:?}",
        );
        // Widen the first struct declaration by one character — every byte
        // from `struct Container` onward shifts, but no line moves.
        let edited = MEMBER_SOURCE.replacen("struct Widget", "struct  Widget", 1);
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&edited);
        {
            let document = backend.documents.get(&uri).expect("open");
            assert_ne!(
                document.analyzed_offset(position),
                document.line_index.offset(position),
                "the fixture must skew the inbound conversion at {position:?}",
            );
        }
        let mid_edit = backend
            .completion(params(&uri))
            .await
            .expect("completion mid-edit");
        assert_eq!(
            labels_of(&baseline),
            labels_of(&mid_edit),
            "the receiver still resolves to Widget from the analyzed snapshot",
        );
    }

    // E52×E53, the union case: code-position `Name::` completion resolves the
    // left segment through the SCOPE CHAIN (`namespace_in_scope`, E53) — an
    // analyzed-space lookup born on a branch that never saw E52's conversion,
    // so their composition is the one place the live offset could leak back
    // in. The fixture's enum lives inside a `mod`, reachable only through the
    // mod's own scope: a skewed live offset resolves the cursor into (or past)
    // the LAST function's scope via `scope_at`'s nearest-before fallback,
    // where `Palette` is not a name at all and the answer collapses to empty.
    #[tokio::test]
    async fn path_completion_answers_the_analyzed_snapshot_while_typing() {
        const PATH_SOURCE: &str = "mod colors {\n\tenum Palette { Red, Blue }\n\tfun inside(): i32 {\n\t\tlet pick = Palette::Red;\n\t\t1\n\t}\n}\n\nfun outside(): i32 {\n\tlet unrelated = 100;\n\tunrelated\n}\n";
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend.documents.insert(uri.clone(), document(PATH_SOURCE));
        // Right after `Palette::`, before `Red` — a path trigger inside the mod.
        let position = position_at(PATH_SOURCE, "Palette::Red", 9);
        let params = |uri: &Url| CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };
        let baseline = backend.completion(params(&uri)).await.expect("completion");
        let labels_of = |response: &Option<CompletionResponse>| match response {
            Some(CompletionResponse::Array(items)) => items
                .iter()
                .map(|item| item.label.clone())
                .collect::<Vec<_>>(),
            other => panic!("the array form is expected, got {other:?}"),
        };
        assert!(
            labels_of(&baseline).contains(&"Red".to_string()),
            "the fixture must offer the mod-scoped enum's variants: {baseline:?}",
        );
        // Widen the first line by 80 bytes — the cursor's line/column never
        // move, so the live trigger scan still finds the same `::` and the
        // same `Palette`, isolating the divergence to the scope resolution.
        let edited =
            PATH_SOURCE.replacen("mod colors", &format!("mod {}colors", "x".repeat(80)), 1);
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&edited);
        {
            let document = backend.documents.get(&uri).expect("open");
            assert_ne!(
                document.analyzed_offset(position),
                document.line_index.offset(position),
                "the fixture must skew the inbound conversion at {position:?}",
            );
        }
        let mid_edit = backend
            .completion(params(&uri))
            .await
            .expect("completion mid-edit");
        assert_eq!(
            labels_of(&baseline),
            labels_of(&mid_edit),
            "the mod-scoped namespace still resolves from the analyzed snapshot",
        );
    }

    // E52: the `.`/`?.`/`::` trigger scan itself must stay LIVE — the fix
    // converts only the downstream scope/entity lookups. A `.` typed on a line
    // that exists ONLY in the live buffer (appended after the last analysis
    // landed) must still be recognized as a member trigger and reach the
    // member path, falling back through `same_file_variable` for the receiver
    // (the salvage path `receiver_nominal_id` documents) rather than silently
    // taking the scope-completion branch.
    #[tokio::test]
    async fn completion_after_a_dot_typed_on_a_live_only_line_still_offers_members() {
        const BASE: &str = "struct Widget {\n\tsize: i32,\n}\n\nfun main() {\n\tlet item = Widget { size = 1 };\n}\n";
        let live = BASE.replacen(
            "\tlet item = Widget { size = 1 };\n}\n",
            "\tlet item = Widget { size = 1 };\n\titem.\n}\n",
            1,
        );
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend.documents.insert(uri.clone(), document(BASE));
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&live);
        let position = position_at(&live, "item.\n", 5);
        {
            let document = backend.documents.get(&uri).expect("open");
            assert!(document.is_stale(), "the buffer is ahead of the analysis");
            assert_ne!(
                document.analyzed_text(),
                document.line_index.text(),
                "the `item.` line must exist only in the live text",
            );
        }
        let response = backend
            .completion(CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            })
            .await
            .expect("completion on a live-only line");
        let labels: Vec<String> = match &response {
            Some(CompletionResponse::Array(items)) => {
                items.iter().map(|item| item.label.clone()).collect()
            }
            other => panic!("the array form is expected, got {other:?}"),
        };
        assert!(
            labels.contains(&"size".to_string()),
            "the trigger scan must still find the `.` in the LIVE text and \
             reach the member path: {labels:?}",
        );
        assert!(
            !labels.contains(&"fun".to_string()),
            "a keyword candidate would mean the dispatch fell through to \
             scope completions instead of the member path: {labels:?}",
        );
    }

    // E66 (editing-dx.md §18), at the protocol layer: the `.` after a CALL's
    // closing paren, typed on a line the analysis has not seen. The trigger
    // scan finds the dot in the live text; the receiver — the call's result,
    // which no `expr_types` entry names — is resolved against the ANALYZED
    // snapshot through `to_analyzed_offset`, so the answer is the result
    // type's members and not the callee's signature.
    #[tokio::test]
    async fn completion_after_a_dot_typed_on_a_call_result_offers_its_members() {
        const BASE: &str = "struct Widget {\n\tsize: i32,\n}\n\n\
             fun build(): Widget { Widget { size = 1 } }\n\n\
             fun main() {\n\tlet _w = build();\n}\n";
        let live = BASE.replacen("\tlet _w = build();\n", "\tlet _w = build().\n", 1);
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend.documents.insert(uri.clone(), document(BASE));
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&live);
        let position = position_at(&live, "build().\n", 8);
        {
            let document = backend.documents.get(&uri).expect("open");
            assert!(document.is_stale(), "the buffer is ahead of the analysis");
        }
        let response = backend
            .completion(CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            })
            .await
            .expect("completion after a call");
        let labels: Vec<String> = match &response {
            Some(CompletionResponse::Array(items)) => {
                items.iter().map(|item| item.label.clone()).collect()
            }
            other => panic!("the array form is expected, got {other:?}"),
        };
        assert!(
            labels.contains(&"size".to_string()),
            "the CALL's result type carries the members: {labels:?}",
        );
        assert!(
            !labels.contains(&"build".to_string()),
            "the callee is not the receiver: {labels:?}",
        );
    }

    // E67 (editing-dx.md §18), at the protocol layer: a `.` typed inside an
    // element's opening tag. The head context is read from a raw parse of the
    // LIVE buffer — element syntax is desugared before analysis, so no element
    // ever reaches `program` and the analyzed snapshot cannot answer this —
    // while the `View` methods themselves come from the analyzed program.
    #[tokio::test]
    async fn completion_after_a_dot_in_an_element_head_offers_the_view_methods() {
        const BASE: &str = "import std::ui::view;\n\nfun main() {\n\t<div></div>\n}\n";
        let live = BASE.replacen("\t<div></div>\n", "\t<div .></div>\n", 1);
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend.documents.insert(uri.clone(), document(BASE));
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&live);
        let position = position_at(&live, "<div .>", 6);
        {
            let document = backend.documents.get(&uri).expect("open");
            assert!(document.is_stale(), "the buffer is ahead of the analysis");
        }
        let response = backend
            .completion(CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            })
            .await
            .expect("completion in an element head");
        let labels: Vec<String> = match &response {
            Some(CompletionResponse::Array(items)) => {
                items.iter().map(|item| item.label.clone()).collect()
            }
            other => panic!("the array form is expected, got {other:?}"),
        };
        assert!(
            labels.contains(&"bind_each".to_string()) && labels.contains(&"text".to_string()),
            "the View chain's methods: {labels:?}",
        );
        assert!(
            !labels.contains(&"attributes".to_string()),
            "a View FIELD is not a chain link: {labels:?}",
        );
        assert!(
            !labels.contains(&"fun".to_string()),
            "a keyword candidate would mean the head context was missed: {labels:?}",
        );
    }

    // S4, pin 12: an analysis that finishes AFTER the document was closed is
    // dropped. Re-inserting it would resurrect a closed buffer — diagnostics
    // reappearing with no document behind them and nothing left to clear them,
    // since `did_close` has already run its clear.
    #[test]
    fn an_analysis_that_finishes_after_a_close_is_dropped() {
        let documents: DashMap<Url, Document> = DashMap::new();
        assert!(
            !land(&documents, &uri(), document(SOURCE)),
            "nothing to land onto",
        );
        assert!(
            documents.is_empty(),
            "the closed document is not resurrected"
        );
    }

    // …while an analysis OF the live text lands: the freshest possible answer,
    // adopted.
    #[test]
    fn an_analysis_of_the_live_text_lands() {
        let documents: DashMap<Url, Document> = DashMap::new();
        let uri = uri();
        documents.insert(uri.clone(), document(SOURCE));
        documents.get_mut(&uri).expect("open").set_text(EDITED);
        assert!(land(&documents, &uri, document(EDITED)));
        let document = documents.get(&uri).expect("still open");
        assert_eq!(document.analyzed_text(), EDITED, "the analysis is adopted");
        assert!(!document.is_stale());
    }

    // S4's ordering half: two analyses of one document can be in flight at once
    // (the debounce generation is only checked before an analysis STARTS), and
    // the OLDER one can finish second. Landing it would regress the analyzed
    // snapshot underneath the newer one and leave the document stuck stale —
    // answers in last-keystroke-but-one coordinates and every rename refused —
    // with nothing scheduled to heal it until the next keystroke. So a result
    // for a text that is no longer the live one is dropped: whatever made the
    // live text move on has its own debounced task (or already landed).
    #[test]
    fn a_stale_analysis_finishing_out_of_order_is_dropped() {
        let documents: DashMap<Url, Document> = DashMap::new();
        let uri = uri();
        documents.insert(uri.clone(), document(SOURCE));
        // The user types EDITED; its analysis (the newer one) lands first…
        documents.get_mut(&uri).expect("open").set_text(EDITED);
        assert!(land(&documents, &uri, document(EDITED)));
        // …and the analysis of the ORIGINAL text (started earlier, finished
        // later) arrives afterwards.
        assert!(
            !land(&documents, &uri, document(SOURCE)),
            "an out-of-order result is dropped",
        );
        let document = documents.get(&uri).expect("still open");
        assert_eq!(
            document.analyzed_text(),
            EDITED,
            "the newer snapshot is not regressed",
        );
        assert!(
            !document.is_stale(),
            "the document does not get stuck refusing renames",
        );
    }

    // The mid-burst variant of the same rule: the buffer has already moved past
    // the text this analysis consumed, so it is dropped even though it is newer
    // than the ADOPTED snapshot — the keystroke that moved the buffer has its
    // own debounced task, and adopting here would still leave the document
    // stale. The analyzed snapshot only ever advances to the live text.
    #[test]
    fn an_analysis_the_buffer_has_moved_past_is_dropped() {
        let documents: DashMap<Url, Document> = DashMap::new();
        let uri = uri();
        documents.insert(uri.clone(), document(SOURCE));
        documents
            .get_mut(&uri)
            .expect("open")
            .set_text("fun main() {\n\tlet value = 2;\n}\n");
        assert!(
            !land(&documents, &uri, document(EDITED)),
            "EDITED is not the live text",
        );
        let document = documents.get(&uri).expect("still open");
        assert_eq!(
            document.text, "fun main() {\n\tlet value = 2;\n}\n",
            "the live edit is kept",
        );
        assert_eq!(
            document.analyzed_text(),
            SOURCE,
            "the snapshot stays consistent at the last adopted analysis",
        );
    }

    // E117, the ghost diagnostic. The two guards above are both text
    // comparisons, and text is exactly what a DEPENDENT's buffer does not
    // change: an edit in a module it imports leaves this file byte-identical,
    // so both of its in-flight analyses match the live text and both land — in
    // whichever order they finish. The one that read the module mid-edit could
    // therefore land, and publish, after the one that read it restored, which
    // is the error that flashes back after a comment/uncomment round trip. The
    // world revision each analysis READ is what separates them.
    #[test]
    fn an_analysis_of_an_older_world_is_dropped_though_its_text_still_matches() {
        let documents: DashMap<Url, Document> = DashMap::new();
        let uri = uri();
        documents.insert(uri.clone(), document(SOURCE));
        // The analysis that read the RESTORED world finishes first.
        let mut newer = document(SOURCE);
        newer.stamp_analysis(5);
        assert!(land(&documents, &uri, newer), "the later world lands");
        // The one that read the module mid-edit finishes second.
        let mut older = document(SOURCE);
        older.stamp_analysis(3);
        assert!(
            !land(&documents, &uri, older),
            "an older world is dropped even though the buffer never moved",
        );
        assert_eq!(
            documents.get(&uri).expect("still open").analysis_revision(),
            5,
            "the adopted snapshot is not regressed to the superseded world",
        );
    }

    // …and the ordering is STRICT on older only: two analyses stamped with the
    // same world say the same thing, and a dependents' sweep legitimately
    // re-runs a document within one world. Dropping an equal stamp would make
    // that sweep a no-op and leave the dependent's diagnostics stale.
    #[test]
    fn an_analysis_of_the_same_world_still_lands() {
        let documents: DashMap<Url, Document> = DashMap::new();
        let uri = uri();
        let mut opened = document(SOURCE);
        opened.stamp_analysis(4);
        documents.insert(uri.clone(), opened);
        let mut resweep = document(SOURCE);
        resweep.stamp_analysis(4);
        assert!(land(&documents, &uri, resweep));
    }
}

/// kolt.local 034: the cross-document reach of references and rename at the
/// HANDLER level. The union itself is pinned on `Document` (references.rs);
/// these pin that the handlers actually hand every open document to it — a
/// handler that quietly answers from one program alone reddens here while the
/// document pins stay green.
#[cfg(test)]
mod cross_document_reach_tests {
    use super::*;
    use crate::document::tests::{analyze_workspace, std_root};
    use snapshot_consistency_tests::{backend, rename_params};

    const LIBRARY: &str = "struct Point {\n\tx: i32,\n}\n";
    const APPLICATION: &str =
        "import pkg::library::Point;\n\nfun main(): i32 {\n\tlet p = Point { x = 1 };\n\tp.x\n}\n";

    /// `library.vl` (the definer) and `application.vl` (its importer), both
    /// open on one backend. Returns the temp dir and each file's URI.
    fn open_pair(backend: &Backend) -> (std::path::PathBuf, Url, Url) {
        let (dir, library_document) =
            analyze_workspace(&[("library.vl", LIBRARY), ("application.vl", APPLICATION)]);
        let application_path = dir.join("application.vl");
        let application_document = Document::analyze(APPLICATION, &std_root(), &application_path);
        let library_uri = Url::from_file_path(dir.join("library.vl")).expect("an absolute path");
        let application_uri = Url::from_file_path(&application_path).expect("an absolute path");
        backend
            .documents
            .insert(library_uri.clone(), library_document);
        backend
            .documents
            .insert(application_uri.clone(), application_document);
        (dir, library_uri, application_uri)
    }

    /// Inside `Point` on line 0 of `library.vl` (`struct Point {`).
    fn declaration_position() -> Position {
        Position::new(0, 9)
    }

    #[tokio::test]
    async fn find_references_reaches_the_open_files_that_import_the_definer() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (dir, library_uri, application_uri) = open_pair(backend);
        let locations = backend
            .references(ReferenceParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: library_uri.clone(),
                    },
                    position: declaration_position(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            })
            .await
            .expect("references answers")
            .expect("an answer for an open document");
        let count = |uri: &Url| {
            locations
                .iter()
                .filter(|location| location.uri == *uri)
                .count()
        };
        assert_eq!(
            count(&library_uri),
            1,
            "the declaration, exactly once: {locations:?}",
        );
        assert_eq!(
            count(&application_uri),
            2,
            "the import leaf and the constructor: {locations:?}",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rename_at_a_definition_rewrites_the_open_files_that_import_it() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let (dir, library_uri, application_uri) = open_pair(backend);
        let edit = backend
            .rename(rename_params(&library_uri, declaration_position()))
            .await
            .expect("the rename answers")
            .expect("`Point` is renameable");
        let changes = edit.changes.expect("per-file edits");
        assert_eq!(changes[&library_uri].len(), 1, "the declaration");
        assert_eq!(
            changes[&application_uri].len(),
            2,
            "the import leaf and the constructor",
        );
        // Every edit replaces exactly the identifier, in each file's own text.
        for (uri, edits) in &changes {
            let text = if *uri == library_uri {
                LIBRARY
            } else {
                APPLICATION
            };
            let index = LineIndex::new(text);
            for edit in edits {
                let start = index.offset(edit.range.start);
                let end = index.offset(edit.range.end);
                assert_eq!(&text[start..end], "Point", "in {uri}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(DashMap::new()),
        semantic_token_cache: Arc::new(DashMap::new()),
        manifests: Arc::new(DashMap::new()),
        publish_state: Arc::new(std::sync::Mutex::new(PublishState::new())),
        pending: Arc::new(DashMap::new()),
        line_indices: Arc::new(DashMap::new()),
        config: Arc::new(std::sync::RwLock::new(Config::default())),
        snippet_support: Arc::new(AtomicBool::new(false)),
        revision: Arc::new(AtomicU64::new(0)),
        publish_gate: Arc::new(tokio::sync::Mutex::new(())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// B39b: the delta path's protocol contract — a full answer carries a
/// `result_id`, a delta request echoing it gets EDITS (zero for an unchanged
/// document), and an unknown baseline re-synchronizes with a full stream.
#[cfg(test)]
mod semantic_token_delta_protocol_tests {
    use super::snapshot_consistency_tests::{SOURCE, backend, open_with_live_edit};
    use super::*;

    fn full_params(uri: &Url) -> SemanticTokensParams {
        SemanticTokensParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    fn delta_params(uri: &Url, previous: &str) -> SemanticTokensDeltaParams {
        SemanticTokensDeltaParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            previous_result_id: previous.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    #[tokio::test]
    async fn a_full_answer_carries_an_id_and_its_delta_is_empty_when_nothing_moved() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, SOURCE);

        let full = backend
            .semantic_tokens_full(full_params(&uri))
            .await
            .unwrap()
            .expect("tokens for an analyzed document");
        let SemanticTokensResult::Tokens(tokens) = full else {
            panic!("expected a token stream");
        };
        assert!(!tokens.data.is_empty(), "the fixture has names to paint");
        let id = tokens.result_id.expect("full now carries a result id");

        let delta = backend
            .semantic_tokens_full_delta(delta_params(&uri, &id))
            .await
            .unwrap()
            .expect("a delta answer");
        match delta {
            SemanticTokensFullDeltaResult::TokensDelta(delta) => {
                assert!(delta.edits.is_empty(), "nothing moved, nothing shipped");
                assert!(delta.result_id.is_some(), "the chain continues");
            }
            other => panic!("expected a delta, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unknown_baseline_resynchronizes_with_a_full_stream() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, SOURCE);

        let answer = backend
            .semantic_tokens_full_delta(delta_params(&uri, "never-issued"))
            .await
            .unwrap()
            .expect("an answer either way");
        match answer {
            SemanticTokensFullDeltaResult::Tokens(tokens) => {
                assert!(!tokens.data.is_empty());
                assert!(tokens.result_id.is_some(), "the resync starts a chain");
            }
            other => panic!("expected a full resync, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_range_request_answers_only_its_lines() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, SOURCE);

        let whole = backend
            .semantic_tokens_range(SemanticTokensRangeParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                range: Range::new(Position::new(0, 0), Position::new(99, 0)),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .unwrap()
            .expect("tokens");
        let SemanticTokensRangeResult::Tokens(whole) = whole else {
            panic!("expected tokens");
        };
        let first_line_only = backend
            .semantic_tokens_range(SemanticTokensRangeParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                range: Range::new(Position::new(0, 0), Position::new(0, 99)),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .unwrap()
            .expect("tokens");
        let SemanticTokensRangeResult::Tokens(first_line_only) = first_line_only else {
            panic!("expected tokens");
        };
        assert!(
            first_line_only.data.len() < whole.data.len(),
            "a one-line window must answer fewer tokens than the document ({} vs {})",
            first_line_only.data.len(),
            whole.data.len()
        );
        assert!(
            first_line_only
                .data
                .iter()
                .all(|token| token.delta_line == 0),
            "everything answered sits on the requested line"
        );
    }
}

/// E122: `semantic_tokens_range` — the request an editor sends most, because it
/// is the VIEWPORT one — used to compute the whole file's token stream and then
/// filter it by line, so twenty visible lines cost exactly what the whole file
/// cost (12.2 ms on kolt's `views.vl`, `proposal/editor-latency.md` §1.6).
///
/// E121's keystroke path moved `full` and the delta onto the analysis's own
/// capture and left THIS request on the walk, so the cost survived the
/// keystroke path intact: on the merged tree, before this fold, the gate below
/// read **0.851×** (20 lines 1752.71 ms, whole file 2059.89 ms of thread CPU
/// over 40 rounds, 12,000 tokens, loadavg 1.29). The request now SLICES E121's
/// capture (`LandedSnapshot::tokens`) through the line index built beside it
/// when the analysis landed — one capture, one invalidation point
/// (`Document::adopt_analysis`), no second memo of the same tokens.
///
/// E125 then moved the request's COORDINATES: it now answers through the same
/// two-sided anchor `full` does, so a viewport is the window of `full`'s own
/// stream instead of a second picture positioned against the analyzed snapshot.
///
/// Four pins over three halves of the claim. The answer did not change:
/// byte-identical to the filter it replaces over every window shape, and
/// byte-identical across an adoption that retains B38's salvage tail — the one
/// shape a single analysis cannot reach, and the one the fold had to repair to
/// serve `full` and `range` from a single capture. The answer FOLLOWS THE
/// BUFFER: a viewport below an unlanded edit is `full`'s window byte for byte,
/// and is not what the pre-E125 mechanism answered. And the cost follows the
/// WINDOW rather than the file.
#[cfg(test)]
mod semantic_token_range_cost_tests {
    use super::snapshot_consistency_tests::backend;
    use super::*;
    use crate::document::tests::std_root;

    /// A module of `functions` four-line functions — the synthetic series
    /// `editor-latency.md` §1.4 scales with, in one file, so the token stream
    /// is large and the viewport is a fixed twenty lines of it.
    fn synthetic_module(functions: usize) -> String {
        let mut text = String::with_capacity(functions * 60);
        for index in 0..functions {
            text.push_str(&format!(
                "fun subject_{index}(input: i32): i32 {{\n\tlet doubled = input + input;\n\tdoubled\n}}\n"
            ));
        }
        text
    }

    fn open(backend: &Backend, text: &str) -> Url {
        let uri = Url::parse("file:///range/subject.vl").expect("a url");
        backend.documents.insert(
            uri.clone(),
            Document::analyze(text, &std_root(), Path::new("subject.vl")),
        );
        uri
    }

    async fn range(backend: &Backend, uri: &Url, first: u32, last: u32) -> Vec<SemanticToken> {
        let answer = backend
            .semantic_tokens_range(SemanticTokensRangeParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                range: Range::new(Position::new(first, 0), Position::new(last, 0)),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .unwrap()
            .expect("a range answer");
        let SemanticTokensRangeResult::Tokens(tokens) = answer else {
            panic!("expected tokens");
        };
        tokens.data
    }

    /// The calling thread's CPU time — the cycles it was actually given, not
    /// the time that passed (backlog M15, `perf_baseline.rs`'s clock). A ratio
    /// on wall clock is a claim about the compiler only on an idle machine, and
    /// this one is gated on a box that runs a dozen lanes at once.
    #[cfg(unix)]
    fn thread_cpu_now() -> Option<Duration> {
        let mut timespec = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `clock_gettime` writes the `timespec` we hand it and reads
        // nothing else; the pointer is to a live local.
        let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut timespec) };
        (result == 0).then(|| {
            Duration::new(
                timespec.tv_sec.max(0) as u64,
                timespec.tv_nsec.clamp(0, 999_999_999) as u32,
            )
        })
    }

    /// Every other host: no thread CPU clock, so the gate below says so and
    /// asserts nothing rather than asserting on wall time.
    #[cfg(not(unix))]
    fn thread_cpu_now() -> Option<Duration> {
        None
    }

    fn loadavg_1m() -> String {
        std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|text| text.split_whitespace().next().map(str::to_string))
            .unwrap_or_else(|| "?".to_string())
    }

    /// Half one: the answer is unchanged. The expected stream is written the
    /// way the handler used to compute it — the WHOLE stream, filtered by the
    /// start line of each token, then encoded — so this compares the slice
    /// against the filter it replaced, byte for byte, over windows that start
    /// mid-file, end past the end, land on a line with no tokens, and cover
    /// everything.
    #[tokio::test]
    async fn a_window_answers_byte_for_byte_what_filtering_the_full_stream_answers() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let text = synthetic_module(40);
        let uri = open(backend, &text);

        for (first, last) in [(0, 0), (0, 19), (7, 7), (12, 31), (100, 400), (0, 100_000)] {
            let sliced = range(backend, &uri, first, last).await;
            let expected = {
                let document = backend.documents.get(&uri).expect("open");
                let index = document.analyzed_index();
                let filtered: Vec<_> = document
                    .semantic_tokens()
                    .into_iter()
                    .filter(|(span, _, _)| {
                        let line = index.range(span).start.line;
                        line >= first && line <= last
                    })
                    .collect();
                encode_semantic_tokens(&filtered, index)
            };
            assert_eq!(
                sliced, expected,
                "lines {first}..={last}: the slice and the filter must answer identically",
            );
        }
        // Non-vacuous: the windows above are not all the same answer.
        let narrow = range(backend, &uri, 0, 3).await;
        let whole = range(backend, &uri, 0, 100_000).await;
        assert!(
            !narrow.is_empty() && narrow.len() < whole.len(),
            "the fixture must have tokens inside AND outside the narrow window \
             ({} vs {})",
            narrow.len(),
            whole.len(),
        );
    }

    /// The same equality through the shape the handler cannot reach by
    /// analyzing once: a document that has ADOPTED a truncating analysis and
    /// is serving B38's salvage tail. The tail is folded into the capture at
    /// adoption (`Document::adopt_analysis`), so the slice covers it; without
    /// that fold the handler would answer nothing below the break while the
    /// filter answered the salvaged tokens.
    #[tokio::test]
    async fn a_window_answers_the_filter_across_a_salvaged_adoption() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let whole = "fun alpha() {\n\tlet a = 1;\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        // An unterminated interpolated triple-quoted string: the lexer stops
        // there, so the analysis is truncated to a prefix and the byte-identical
        // tail below it is what B38 retains.
        let broken = "fun alpha() {\n\tlet a = i\"\"\";\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        let uri = open(backend, whole);
        {
            let mut document = backend.documents.get_mut(&uri).expect("open");
            document.adopt_analysis(Document::analyze(
                broken,
                &std_root(),
                Path::new("subject.vl"),
            ));
            // The state a server is actually in once that analysis lands: the
            // buffer holds the text it ran on. E125 answers a viewport against
            // the LIVE buffer, so leaving the document with `whole` in it and
            // `broken` analyzed would compare a live answer with an analyzed
            // filter — a difference about the ANCHOR, which is the next pin's
            // subject, not this one's. Here the two coincide and the equality
            // is about the salvage fold alone.
            document.set_text(broken);
        }

        for (first, last) in [(0, 0), (4, 4), (3, 5), (0, 100_000)] {
            let sliced = range(backend, &uri, first, last).await;
            let expected = {
                let document = backend.documents.get(&uri).expect("open");
                let index = document.analyzed_index();
                let filtered: Vec<_> = document
                    .semantic_tokens()
                    .into_iter()
                    .filter(|(span, _, _)| {
                        let line = index.range(span).start.line;
                        line >= first && line <= last
                    })
                    .collect();
                encode_semantic_tokens(&filtered, index)
            };
            assert_eq!(
                sliced, expected,
                "lines {first}..={last}: the slice and the filter must answer \
                 identically across a salvaged adoption",
            );
        }
        // Non-vacuous: line 4 is `let zeta = 9;`, below the break, and it is
        // ANSWERED — that is the salvage this pin is about.
        assert!(
            !range(backend, &uri, 4, 4).await.is_empty(),
            "the salvaged tail line must still be painted, or the equality \
             above holds because both sides are empty",
        );
    }

    /// E125: a viewport answered after an UNLANDED EDIT ABOVE IT agrees with
    /// `semanticTokens/full`'s own window, byte for byte.
    ///
    /// This is the drift the keystroke path exists to remove, on the request an
    /// editor sends most. `full` re-serves the landed capture through the
    /// two-sided anchor, so it is positioned against the buffer on screen;
    /// `range` sliced the same capture by ANALYZED line and encoded it against
    /// the analyzed index, so every token below an inserted line was painted
    /// one line high until the next analysis landed. Two requests, one
    /// capture, two pictures.
    ///
    /// The edit is a whole line inserted inside a function BODY, which is the
    /// shape the anchor is built for: the declaration-shape stamp does not
    /// move, so the verdict is `Exact` and the landed classification below the
    /// edit is still true — it has only moved down a line.
    ///
    /// **Non-vacuous by construction**: the pre-E125 mechanism is computed
    /// beside the answer (the capture sliced by analyzed line, encoded against
    /// the analyzed index) and asserted to DIFFER. If the anchor ever became a
    /// no-op here, that assertion reds before the equality does.
    #[tokio::test]
    async fn a_viewport_follows_an_unlanded_edit_above_it() {
        const FUNCTIONS: usize = 40;
        // The viewport, in LIVE lines — far enough below the edit that every
        // token in it comes from the anchor's TAIL.
        const FIRST: u32 = 100;
        const LAST: u32 = 119;

        let (service, _socket) = backend();
        let backend = service.inner();
        let analyzed = synthetic_module(FUNCTIONS);
        let uri = open(backend, &analyzed);
        // One line typed into the FIRST function's body, above the viewport.
        let live = analyzed.replacen(
            "\tlet doubled = input + input;\n",
            "\tlet doubled = input + input;\n\tlet spare = input;\n",
            1,
        );
        assert_eq!(
            live.lines().count(),
            analyzed.lines().count() + 1,
            "the edit must add exactly one line above the viewport",
        );
        backend
            .documents
            .get_mut(&uri)
            .expect("open")
            .set_text(&live);

        let sliced = range(backend, &uri, FIRST, LAST).await;
        let (full_window, stale_mechanism) = {
            let document = backend.documents.get(&uri).expect("open");
            let live_index = &document.line_index;
            let window: Vec<_> = document
                .keystroke_tokens(false)
                .into_iter()
                .filter(|(span, ..)| {
                    let line = live_index.range(span).start.line;
                    (FIRST..=LAST).contains(&line)
                })
                .collect();
            // The pre-E125 mechanism, written the way its own pins write it:
            // the capture selected by ANALYZED line and encoded against the
            // analyzed index.
            let analyzed_index = document.analyzed_index();
            let stale: Vec<_> = document
                .semantic_tokens()
                .into_iter()
                .filter(|(span, ..)| {
                    let line = analyzed_index.range(span).start.line;
                    (FIRST..=LAST).contains(&line)
                })
                .collect();
            (
                encode_semantic_tokens(&window, live_index),
                encode_semantic_tokens(&stale, analyzed_index),
            )
        };
        assert!(
            !sliced.is_empty(),
            "the viewport must hold tokens, or the equality below is vacuous",
        );
        assert_eq!(
            sliced, full_window,
            "a viewport after an unlanded edit above it must be exactly \
             `full`'s window — one capture, one picture (E125)",
        );
        assert_ne!(
            sliced, stale_mechanism,
            "the pre-E125 mechanism — the capture sliced by ANALYZED line — \
             answered the same bytes here, so this pin proves nothing about \
             the anchor",
        );
    }

    /// Half two: the cost follows the window. Same handler, same document, same
    /// warm stream — only the window differs, and a twenty-line viewport must
    /// cost a small fraction of the whole file.
    ///
    /// Non-vacuous, measured on both sides of the fold on the MERGED tree:
    /// planting the pre-fold mechanism — the walk plus a per-token line
    /// filter, which is what E121's keystroke path left this one request on —
    /// reads **0.851×** (20 lines 1752.71 ms, whole file 2059.89 ms of thread
    /// CPU over 40 rounds, loadavg 1.29), and the slice reads **0.003×**
    /// (0.98 ms vs 286.26 ms). The 0.25 bound sits between two MECHANISMS, not two
    /// tunings: no recompute-then-filter form can get under it, and the slice
    /// has forty times the headroom it needs.
    #[tokio::test]
    async fn a_viewport_costs_the_viewport_and_not_the_file() {
        const FUNCTIONS: usize = 1_500;
        const ROUNDS: usize = 40;
        const BOUND: f64 = 0.25;

        let (service, _socket) = backend();
        let backend = service.inner();
        let text = synthetic_module(FUNCTIONS);
        let lines = text.lines().count() as u32;
        let uri = open(backend, &text);
        // Warm the per-analysis stream, so what follows measures SERVING a
        // window and not building the stream. An editor's first request pays
        // this once per analysis whatever window it asks for; every request
        // after it is what this gate is about.
        let all = range(backend, &uri, 0, lines).await;
        assert!(
            all.len() > FUNCTIONS * 4,
            "the synthetic module must produce a large stream for the ratio to \
             mean anything (got {})",
            all.len(),
        );

        let Some(start) = thread_cpu_now() else {
            println!("PERF-SCALE semantic_tokens_range clock=none: no thread CPU clock, not gated");
            return;
        };
        for _ in 0..ROUNDS {
            let _ = range(backend, &uri, 100, 119).await;
        }
        let viewport = thread_cpu_now().expect("the clock does not disappear") - start;
        let start = thread_cpu_now().expect("the clock does not disappear");
        for _ in 0..ROUNDS {
            let _ = range(backend, &uri, 0, lines).await;
        }
        let file = thread_cpu_now().expect("the clock does not disappear") - start;

        let ratio = viewport.as_secs_f64() / file.as_secs_f64().max(f64::MIN_POSITIVE);
        println!(
            "PERF-SCALE semantic_tokens_range clock=thread load={} \
             {FUNCTIONS} functions / {lines} lines / {} tokens, {ROUNDS} rounds: \
             20 lines = {:.2} ms, whole file = {:.2} ms, ratio {ratio:.3}×",
            loadavg_1m(),
            all.len(),
            viewport.as_secs_f64() * 1000.0,
            file.as_secs_f64() * 1000.0,
        );
        assert!(
            file > Duration::ZERO,
            "the whole-file measurement read zero, so the ratio means nothing",
        );
        assert!(
            ratio <= BOUND,
            "a 20-line viewport cost {ratio:.3}× the whole file ({:.2} ms vs \
             {:.2} ms of thread CPU over {ROUNDS} rounds, loadavg {}), over the \
             {BOUND} bound: the range request is paying for the file again \
             (editor-latency.md §1.6, E122)",
            viewport.as_secs_f64() * 1000.0,
            file.as_secs_f64() * 1000.0,
            loadavg_1m(),
        );
    }
}

/// B39c: incremental sync through the handler — ordered ranged events
/// rebuild the text a full sync would have sent (documents and manifests
/// both), and the inlay viewport filter goes EXACT when the edit map holds:
/// a line inserted above moves the hint's live line, and the filter follows
/// it instead of answering the stale window.
#[cfg(test)]
mod incremental_sync_tests {
    use super::snapshot_consistency_tests::{SOURCE, backend};
    use super::*;

    fn change(line: u32, start: u32, end: u32, text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(Range::new(
                Position::new(line, start),
                Position::new(line, end),
            )),
            range_length: None,
            text: text.to_string(),
        }
    }

    fn uri() -> Url {
        Url::parse("file:///incremental/main.vl").unwrap()
    }

    #[tokio::test]
    async fn ranged_events_rebuild_the_document_text() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        backend.documents.insert(
            uri.clone(),
            crate::document::Document::analyze(
                SOURCE,
                &crate::document::tests::std_root(),
                std::path::Path::new("/incremental/main.vl"),
            ),
        );
        backend
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![
                    // `let value = 1;` -> `let value = 9;`, then append a
                    // comment on the next event AGAINST THE EDITED TEXT.
                    change(1, 13, 14, "9"),
                    change(0, 12, 12, " // edited"),
                ],
            })
            .await;
        let text = backend.documents.get(&uri).unwrap().text.clone();
        assert_eq!(
            text,
            SOURCE
                .replace("= 1;", "= 9;")
                .replace("fun main() {", "fun main() { // edited"),
            "ordered ranged events rebuild what full sync would have sent"
        );
    }

    #[tokio::test]
    async fn a_manifest_folds_ranged_events_too() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = Url::parse("file:///incremental/vilan.toml").unwrap();
        backend
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 1,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "[package]\nname = \"a\"\n".to_string(),
                }],
            })
            .await;
        backend
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![change(1, 8, 9, "b")],
            })
            .await;
        let text = backend.manifests.get(&uri).unwrap().text.clone();
        assert_eq!(text, "[package]\nname = \"b\"\n");
    }

    #[tokio::test]
    async fn the_viewport_filter_follows_a_hint_moved_by_an_edit_above() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = uri();
        let document = crate::document::Document::analyze(
            SOURCE,
            &crate::document::tests::std_root(),
            std::path::Path::new("/incremental/main.vl"),
        );
        backend.documents.insert(uri.clone(), document);

        // The fixture's hints sit on lines 1 and 2 (the two lets). The LAST
        // one makes the pin sharp: no other hint's unmapped position can
        // reach the window below it, so only the real mapping answers.
        let hint_line = {
            let document = backend.documents.get(&uri).unwrap();
            document
                .inlay_hints()
                .into_iter()
                .map(|(offset, _)| document.analyzed_position(offset).line)
                .max()
                .expect("a hint")
        };

        // Insert a whole line above via the handler: the hint's LIVE line is
        // now one greater, with no analysis landed.
        backend
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![change(0, 0, 0, "// pushed down\n")],
            })
            .await;

        let ask = |line: u32| InlayHintParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range::new(Position::new(line, 0), Position::new(line, 999)),
            work_done_progress_params: Default::default(),
        };
        let at_live_line = backend
            .inlay_hint(ask(hint_line + 1))
            .await
            .unwrap()
            .unwrap_or_default();
        assert!(
            !at_live_line.is_empty(),
            "the hint's live line answers it - the exact filter follows the edit"
        );
        let at_stale_line_only = backend
            .inlay_hint(ask(hint_line))
            .await
            .unwrap()
            .unwrap_or_default();
        assert!(
            at_stale_line_only
                .iter()
                .all(|hint| hint.position.line != hint_line + 1),
            "the stale window no longer over-answers the moved hint"
        );
    }
}

/// How long a language-server test may wait for an ANALYSIS to land before it
/// calls the server stuck.
///
/// A LIVENESS bound, not a performance assertion: no pin that reads it claims
/// an analysis is fast, only that it arrives without a restart. So the number
/// only has to be too large for a healthy analysis and finite for a stuck one,
/// and a green run never pays it — every reader polls and returns the moment
/// its condition holds.
///
/// The recolor pins' version of it was 10 s (200 × 50 ms), and 10 s is not too
/// large for a healthy sweep (tracker N46): the recolor is a real re-analysis
/// of a real package on a blocking thread, it costs 13.8 s for the whole pin on
/// an idle box, and two sibling lanes' unions turned it red under ten-lane load
/// while the same pin PASSED at 19.7 s at loadavg ~85 and passes on CI. That is
/// E39/E40's disease exactly — `WATCH_LIVENESS` was raised 20 s → 120 s →
/// 300 s for it, one strike at a time — so this takes 300 s at once, for the
/// same recorded reason: the whole point of the bound is to catch work that
/// never fires, and the machine's speed is not what any of these pins is about.
///
/// It is ONE constant because it is one claim. E123 made `did_open` schedule
/// its first analysis instead of running it on the notification handler, which
/// gave three more pins a wall-clock wait of exactly this shape (an open's own
/// analysis, on a blocking thread, on the same box); giving them their own
/// numbers would have been three more claims about how long an analysis may
/// take, and none of them makes such a claim.
#[cfg(test)]
const ANALYSIS_LIVENESS: Duration = Duration::from_secs(300);

/// E106: the scripted session — an open/edit/close loop driven straight through
/// the server's own handlers, asserting that every retained map comes back to
/// where it started.
///
/// The item asks to MEASURE before designing a reclaim, and the owner's decisive
/// datapoint (a language-server restart that did not help, a VS Code restart
/// that did) put the prime suspicion on the editor side. That does not exonerate
/// the server: it only says a leak here would have been cleared by the restart.
/// This is how the server half is held to it — the session's memory is its maps,
/// so a map that does not return to baseline over a loop of open/edit/close is a
/// leak with a name and a count, which the `session_trace` summary then reports
/// to a live session in the same terms.
///
/// `line_indices` is deliberately EXEMPT from the baseline: it is a by-path
/// cache of files that are NOT open (std, and workspace files reached by
/// cross-file navigation), documented as never invalidated, so it is bounded by
/// the project's file count rather than by the session's length. It is still
/// counted in the trace summary, because "bounded by the project" is a claim a
/// climbing number in a real session would refute.
#[cfg(test)]
mod session_leak_tests {
    use super::snapshot_consistency_tests::backend;
    use super::*;

    const PROGRAM: &str = "fun main() {\n\tlet value = 1;\n\tlet other = value;\n}\n";

    pub(crate) fn open_params(uri: &Url, text: &str) -> DidOpenTextDocumentParams {
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "vilan".to_string(),
                version: 1,
                text: text.to_string(),
            },
        }
    }

    pub(crate) fn whole_file_change(
        uri: &Url,
        version: i32,
        text: &str,
    ) -> DidChangeTextDocumentParams {
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        }
    }

    /// Waits for an open's analysis to land. E123 made an open SCHEDULE its
    /// first analysis instead of running it on the handler, and without this
    /// the scripted session below would open four files, close them, and
    /// never once run the work whose residue it exists to measure — the pin
    /// went from tens of seconds to 88 ms the day the open stopped blocking,
    /// which is exactly the shape of a gate quietly becoming vacuous.
    async fn analyzed(backend: &Backend, uri: &Url) -> bool {
        let deadline = std::time::Instant::now() + ANALYSIS_LIVENESS;
        while std::time::Instant::now() < deadline {
            if backend
                .documents
                .get(uri)
                .is_some_and(|document| document.analysis_revision() > 0)
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    }

    /// Every retained map's cardinality, as one comparable tuple.
    fn sizes(backend: &Backend) -> session_trace::StateSizes {
        session_trace::StateSizes {
            documents: backend.documents.len(),
            semantic_token_cache: backend.semantic_token_cache.len(),
            manifests: backend.manifests.len(),
            pending: backend.pending.len(),
            line_indices: backend.line_indices.len(),
        }
    }

    /// The scripted session: several files, each opened, edited a few times
    /// (with the semantic-token and inlay-hint requests an editor really sends
    /// after an edit), then closed — the whole loop repeated, so a per-round
    /// residue shows up as a multiple rather than as one ambiguous entry.
    #[tokio::test]
    async fn an_open_edit_close_loop_returns_every_retained_map_to_baseline() {
        let directory = std::env::temp_dir().join(format!(
            "vilan-session-leak-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let (service, _socket) = backend();
        let backend = service.inner();

        let baseline = sizes(backend);
        assert_eq!(
            baseline.documents, 0,
            "the session starts holding nothing: {baseline:?}",
        );

        const ROUNDS: usize = 3;
        const FILES: usize = 4;
        for round in 0..ROUNDS {
            let mut open = Vec::new();
            for file in 0..FILES {
                let path = directory.join(format!("session_{file}.vl"));
                std::fs::write(&path, PROGRAM).expect("a source file");
                let uri = Url::from_file_path(&path).expect("a file url");
                backend.did_open(open_params(&uri, PROGRAM)).await;
                assert!(
                    analyzed(backend, &uri).await,
                    "round {round}: the open's analysis lands",
                );
                open.push(uri);
            }
            assert_eq!(
                backend.documents.len(),
                FILES,
                "round {round}: every opened file is held",
            );

            // The requests an editor sends while typing, so the caches an edit
            // fills are actually filled before the close has to reclaim them.
            for (keystroke, uri) in open.iter().enumerate() {
                let edited = format!("{PROGRAM}// edit {keystroke}\n");
                backend
                    .did_change(whole_file_change(uri, 2 + keystroke as i32, &edited))
                    .await;
                let _ = backend
                    .semantic_tokens_full(SemanticTokensParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
                let _ = backend
                    .inlay_hint(InlayHintParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        range: Range::new(Position::new(0, 0), Position::new(20, 0)),
                        work_done_progress_params: Default::default(),
                    })
                    .await;
            }

            for uri in &open {
                backend
                    .did_close(DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                    })
                    .await;
            }

            let after = sizes(backend);
            assert_eq!(
                (
                    after.documents,
                    after.semantic_token_cache,
                    after.manifests,
                    after.pending,
                ),
                (
                    baseline.documents,
                    baseline.semantic_token_cache,
                    baseline.manifests,
                    baseline.pending,
                ),
                "round {round}: closing every document must give the session's \
                 memory back (baseline {baseline:?}, after {after:?})",
            );
        }

        let _ = std::fs::remove_dir_all(&directory);
    }
}

/// E123: `did_open` used to run `Document::analyze` INLINE on the async
/// handler. That call joins a 128 MiB analysis thread, so opening kolt's
/// `views.vl` parked a tokio worker for the whole 1.1 s first analysis and
/// every other request scheduled on that worker waited behind it — the session
/// trace's "slow request: didOpen took 1112 ms"
/// (`proposal/editor-latency.md` §1.6). The open now inserts the buffer and
/// schedules the analysis the way an edit's is scheduled: `spawn_blocking`,
/// stamped with the world it read (E117), landed only if it is still the newest
/// view.
#[cfg(test)]
mod open_scheduling_tests {
    use super::session_leak_tests::open_params;
    use super::snapshot_consistency_tests::backend;
    use super::*;

    /// Big enough that its analysis is unmistakably work, and wrong, so the
    /// diagnostic the open must still publish is unambiguous.
    fn subject() -> String {
        let mut text = String::from("fun main() {\n\tlet value = undefined_name;\n}\n");
        for index in 0..200 {
            text.push_str(&format!(
                "fun helper_{index}(input: i32): i32 {{\n\tinput + input\n}}\n"
            ));
        }
        text
    }

    fn workspace(name: &str) -> (PathBuf, Url) {
        let directory = std::env::temp_dir().join(format!(
            "vilan-open-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let path = directory.join("opened.vl");
        std::fs::write(&path, subject()).expect("a source file");
        let uri = Url::from_file_path(&path).expect("a file url");
        (directory, uri)
    }

    /// Polls for the open's analysis to land, rather than sleeping a fixed
    /// span: it is real work on a blocking thread and a loaded machine is
    /// exactly when a fixed sleep turns a pin into a flake
    /// (`package_recolor_tests::settled`'s rule).
    async fn landed(backend: &Backend, uri: &Url) -> bool {
        let deadline = std::time::Instant::now() + ANALYSIS_LIVENESS;
        while std::time::Instant::now() < deadline {
            if backend
                .documents
                .get(uri)
                .is_some_and(|document| document.analysis_revision() > 0)
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    }

    /// The pin. A request issued while a fresh open's analysis is in flight is
    /// ANSWERED — it does not queue behind the analysis — and the open's
    /// diagnostics still publish once it lands.
    ///
    /// `#[tokio::test]` is a current-thread runtime, which is what makes this
    /// exact rather than statistical: the analysis is spawned, so it cannot
    /// have run before the request below is served, and on the pre-fix tree
    /// `did_open().await` did not return until the analysis had landed —
    /// the "not landed yet" assertion is red there by construction.
    #[tokio::test]
    async fn a_request_during_a_fresh_open_answers_before_the_analysis_lands() {
        let (directory, uri) = workspace("inflight");
        let (service, _socket) = backend();
        let backend = service.inner();

        let opened = std::time::Instant::now();
        backend.did_open(open_params(&uri, &subject())).await;
        let open_wall = opened.elapsed();

        // The buffer is in the map immediately: a query finds the document,
        // which is what `land`'s "a missing entry can only mean closed" rests
        // on, and what the ordering of these two facts is about.
        let document = backend.documents.get(&uri).expect("the open registered");
        assert_eq!(
            document.analysis_revision(),
            0,
            "the open's analysis must still be in flight — an open that has \
             already analyzed blocked its worker to do it (E123)",
        );
        drop(document);

        let answering = std::time::Instant::now();
        let answer = backend
            .semantic_tokens_full(SemanticTokensParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("the request is answered, not refused");
        let answer_wall = answering.elapsed();
        assert!(
            answer.is_some(),
            "the handler answers over the open document, empty stream and all",
        );
        assert!(
            backend
                .documents
                .get(&uri)
                .is_some_and(|document| document.analysis_revision() == 0),
            "…and it answered while the analysis was still in flight",
        );

        assert!(landed(backend, &uri).await, "the open's analysis lands");
        let published = backend
            .documents
            .get(&uri)
            .expect("open")
            .published_diagnostics()
            .len();
        assert!(
            published > 0,
            "the open's diagnostics still publish: the subject has an \
             undefined name in it",
        );
        println!(
            "E123 open scheduling: load={} didOpen returned in {:.1} ms, the \
             request answered in {:.1} ms, {published} diagnostics landed after",
            std::fs::read_to_string("/proc/loadavg")
                .ok()
                .and_then(|text| text.split_whitespace().next().map(str::to_string))
                .unwrap_or_else(|| "?".to_string()),
            open_wall.as_secs_f64() * 1000.0,
            answer_wall.as_secs_f64() * 1000.0,
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The other half of "route it through the same scheduling": an edit that
    /// arrives while the open's analysis is still running wins. Both analyses
    /// are stamped with the world they read (E117) and `land` keeps the newer
    /// one, so the document does not settle on the opened text.
    #[tokio::test]
    async fn an_edit_during_a_fresh_open_is_the_analysis_that_settles() {
        let (directory, uri) = workspace("superseded");
        let (service, _socket) = backend();
        let backend = service.inner();

        backend.did_open(open_params(&uri, &subject())).await;
        let edited = format!("{}\nfun added(): i32 {{\n\t1\n}}\n", subject());
        backend
            .did_change(super::session_leak_tests::whole_file_change(
                &uri, 2, &edited,
            ))
            .await;

        let deadline = std::time::Instant::now() + ANALYSIS_LIVENESS;
        while std::time::Instant::now() < deadline {
            let settled = backend.documents.get(&uri).is_some_and(|document| {
                document.analysis_revision() > 0 && document.analyzed_text() == edited
            });
            if settled {
                let _ = std::fs::remove_dir_all(&directory);
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let _ = std::fs::remove_dir_all(&directory);
        panic!("the edit's analysis never became the analyzed snapshot");
    }
}

/// E116: a file's platform color is decided by which ENTRY reaches it, and the
/// reachability walk is per-analysis — so the coloring only moves when the file
/// is re-analyzed, and nothing used to re-analyze a file because SOMEONE ELSE'S
/// import graph changed. The owner's report: an unreferenced file falls back to
/// the process layer (E113's designated-entry rule, correct), then keeps that
/// color after the import that reaches it is written, until the server is
/// restarted.
///
/// Driven through the real notification handlers, because the bug is entirely
/// in which documents the server chooses to re-analyze.
#[cfg(test)]
mod package_recolor_tests {
    use super::session_leak_tests::{open_params, whole_file_change};
    use super::snapshot_consistency_tests::backend;
    use super::*;

    /// The kolt shape: a browser `client`, a node `server`, the process side
    /// designated — so a module NO entry reaches falls back to process.
    const MANIFEST: &str = "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n\
         [entry.client]\ntarget = \"browser\"\n\n[entry.server]\n";
    /// A module using the BROWSER `View`'s `element` field: clean under
    /// `browser`, "no field 'element'" under any process target.
    const WIDGET: &str = "import std::ui::{ View, view };\n\n\
         fun attach(): View {\n\tlet root = view(\"div\");\n\t\
         root.element.set_attribute(\"id\", \"app\");\n\troot\n}\n";
    const CLIENT_WITHOUT_IMPORT: &str =
        "import std::io::print;\n\nfun main() {\n\tprint(\"client\");\n}\n";
    const CLIENT_WITH_IMPORT: &str =
        "import pkg::widget::attach;\n\nfun main() {\n\tattach();\n}\n";
    const SERVER: &str = "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n";

    /// The fixture package on disk, and the URIs of the two files the editor
    /// opens. Uniquified per test process and per thread, like every other
    /// workspace fixture here.
    fn workspace(name: &str) -> (PathBuf, Url, Url) {
        let directory = std::env::temp_dir().join(format!(
            "vilan-recolor-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("src")).expect("a scratch directory");
        for (relative, contents) in [
            ("vilan.toml", MANIFEST),
            ("src/widget.vl", WIDGET),
            ("src/client.vl", CLIENT_WITHOUT_IMPORT),
            ("src/server.vl", SERVER),
        ] {
            std::fs::write(directory.join(relative), contents).expect("a source file");
        }
        let widget = Url::from_file_path(directory.join("src/widget.vl")).expect("a file url");
        let client = Url::from_file_path(directory.join("src/client.vl")).expect("a file url");
        (directory, widget, client)
    }

    /// Whether the open document at `uri` is currently publishing the
    /// process-`View` error — the exact squiggle the owner sees on a
    /// browser-only file colored as process.
    fn colored_as_process(backend: &Backend, uri: &Url) -> bool {
        backend
            .documents
            .get(uri)
            .expect("open")
            .published_diagnostics()
            .iter()
            .any(|item| item.message.contains("has no field 'element'"))
    }

    /// Wait for the debounced analysis and the sweep it triggers to settle.
    /// Polls rather than sleeping a fixed span: the analysis is real work on a
    /// blocking thread, and a loaded machine is exactly when a fixed sleep
    /// turns a pin into a flake.
    async fn settled(backend: &Backend, uri: &Url, expected: bool) -> bool {
        let deadline = std::time::Instant::now() + ANALYSIS_LIVENESS;
        while std::time::Instant::now() < deadline {
            if colored_as_process(backend, uri) == expected {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    /// Waits for an OPEN's own analysis to land (E123: it is scheduled, not
    /// run on the notification handler). Distinct from [`settled`], which polls
    /// a diagnostic: "no error yet" and "no analysis yet" look the same from
    /// there, and the premise assertions below would be vacuous.
    async fn analyzed(backend: &Backend, uri: &Url) -> bool {
        // The file's one `ANALYSIS_LIVENESS`, like [`settled`]: this waits for
        // the same kind of work on the same box — two whole analyses of a
        // two-file package, now started CONCURRENTLY because each open
        // schedules its own (E123).
        let deadline = std::time::Instant::now() + ANALYSIS_LIVENESS;
        while std::time::Instant::now() < deadline {
            if backend
                .documents
                .get(uri)
                .is_some_and(|document| document.analysis_revision() > 0)
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    }

    #[tokio::test]
    async fn a_package_import_edit_recolors_the_open_file_it_reaches() {
        let (directory, widget_uri, client_uri) = workspace("import");
        let (service, _socket) = backend();
        let backend = service.inner();
        backend.did_open(open_params(&widget_uri, WIDGET)).await;
        backend
            .did_open(open_params(&client_uri, CLIENT_WITHOUT_IMPORT))
            .await;
        assert!(
            analyzed(backend, &widget_uri).await && analyzed(backend, &client_uri).await,
            "both opens' analyses land (they are scheduled, not inline — E123), \
             which is the settled starting state this pin edits from",
        );
        assert!(
            colored_as_process(backend, &widget_uri),
            "no entry reaches the widget yet, so `default-entry = \"server\"` colors it process \
             — E113's fallback, and the premise of this pin",
        );

        // The entry gains the import that reaches it. The widget's own buffer
        // does not move, and it does not depend on the entry — the entry
        // depends on IT — so the dependency-edge sweep finds nothing to do.
        backend
            .did_change(whole_file_change(&client_uri, 2, CLIENT_WITH_IMPORT))
            .await;
        assert!(
            settled(backend, &widget_uri, false).await,
            "the error must clear without a restart: the widget is now reached by the browser \
             entry, so it analyzes as browser",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[tokio::test]
    async fn removing_the_import_colors_the_file_back() {
        // The mirror, so the sweep is not one-way: deleting the import makes
        // the widget unreached again and the process fallback returns. A fix
        // that only ever re-colored TOWARD browser would pass the first pin
        // and fail this one.
        let (directory, widget_uri, client_uri) = workspace("removal");
        std::fs::write(directory.join("src/client.vl"), CLIENT_WITH_IMPORT).expect("a source file");
        let (service, _socket) = backend();
        let backend = service.inner();
        backend.did_open(open_params(&widget_uri, WIDGET)).await;
        backend
            .did_open(open_params(&client_uri, CLIENT_WITH_IMPORT))
            .await;
        assert!(
            analyzed(backend, &widget_uri).await && analyzed(backend, &client_uri).await,
            "both opens' analyses land (they are scheduled, not inline — E123), \
             which is the settled starting state this pin edits from",
        );
        assert!(
            !colored_as_process(backend, &widget_uri),
            "the browser entry reaches it, so it starts clean",
        );
        backend
            .did_change(whole_file_change(&client_uri, 2, CLIENT_WITHOUT_IMPORT))
            .await;
        assert!(
            settled(backend, &widget_uri, true).await,
            "unreached again: the designated process entry colors it once more",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The decision itself, without a server. An ordinary body edit leaves the
    /// `pkg::` graph exactly where it was, so it must NOT drag every open file
    /// in the package through a re-analysis — the sweep is the expensive half
    /// of a typing pause, and widening it unconditionally would undo B39a.
    #[test]
    fn an_edit_that_does_not_move_the_graph_recolors_nothing() {
        let root = PathBuf::from("/pkg/src");
        assert_eq!(
            recolored_package(Some((7, root.clone())), Some((7, root.clone()))),
            None,
            "same package, same reach",
        );
        assert_eq!(
            recolored_package(Some((7, root.clone())), Some((9, root.clone()))),
            Some(root.clone()),
            "the reach moved: the whole package is re-colored",
        );
        assert_eq!(
            recolored_package(None, Some((7, root.clone()))),
            Some(root.clone()),
            "a file that had no package and now has one re-colors it",
        );
        assert_eq!(
            recolored_package(Some((7, root)), None),
            None,
            "a file with no package of its own sweeps nobody",
        );
    }
}

/// E112: `line_indices` is a by-path cache of files that are on disk and not
/// buffered, and it had no invalidation — documented as safe because it was
/// "written for `std`, whose files do not change". Stability was never a
/// property of the key. A workspace file is exempt only while a buffer is
/// registered for it, so a file cached before it was ever opened kept its
/// pre-edit index across the whole open/edit/save/close cycle, and every later
/// cross-file reference into it converted spans through the wrong line breaks.
/// Correctness, not perf — a wrong position, published.
#[cfg(test)]
mod line_index_cache_tests {
    use super::snapshot_consistency_tests::backend;
    use super::*;

    /// A scratch file with `contents`, in a directory unique to this test.
    fn scratch(name: &str, contents: &str) -> (PathBuf, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "vilan-line-index-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let path = directory.join("module.vl");
        std::fs::write(&path, contents).expect("a source file");
        (directory, path)
    }

    #[test]
    fn a_file_that_changed_on_disk_is_re_indexed() {
        // One line, then three: the line breaks the index converts through move,
        // which is exactly what a stale entry gets wrong.
        let (directory, path) = scratch("stale", "fun answer(): i32 { 1 }\n");
        let (service, _socket) = backend();
        let backend = service.inner();
        let first = backend.line_index_for(&path).expect("readable");
        assert_eq!(
            first.position(20).line,
            0,
            "the one-line file puts every offset on line 0",
        );
        // The file is rewritten under the server — a save from another window,
        // a `git checkout`, a generator. Nothing notifies it.
        std::fs::write(&path, "fun answer(): i32 {\n\t1\n}\n").expect("a rewrite");
        let second = backend.line_index_for(&path).expect("readable");
        assert_eq!(
            second.position(21).line,
            1,
            "the index must describe the file that is there now, not the one that was",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_unchanged_file_still_answers_from_the_cache() {
        // The other half: the validation must not turn the cache off. An
        // unchanged file answers with the very same `Arc` — no re-read, no
        // re-index, which is the whole reason the map exists.
        let (directory, path) = scratch("cached", "fun answer(): i32 { 1 }\n");
        let (service, _socket) = backend();
        let backend = service.inner();
        let first = backend.line_index_for(&path).expect("readable");
        let second = backend.line_index_for(&path).expect("readable");
        assert!(
            Arc::ptr_eq(&first, &second),
            "an unchanged file is served from the cache",
        );
        assert_eq!(backend.line_indices.len(), 1, "one entry, not two");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_buffered_file_is_never_cached() {
        // Unchanged from before, and re-pinned here because the stamp must not
        // become an excuse to start caching a buffer: its text is one keystroke
        // old on disk, and the overlay is the truth.
        let (directory, path) = scratch("buffered", "fun answer(): i32 { 1 }\n");
        let (service, _socket) = backend();
        let backend = service.inner();
        vilan_core::analyzer::set_document_overlay(&path, Some("fun answer(): i32 { 2 }\n".into()));
        let first = backend.line_index_for(&path).expect("readable");
        let second = backend.line_index_for(&path).expect("readable");
        assert!(!Arc::ptr_eq(&first, &second), "indexed fresh every time");
        assert!(backend.line_indices.is_empty(), "and never stored");
        vilan_core::analyzer::set_document_overlay(&path, None);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
