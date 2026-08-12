//! The Vilan language server: a thin tower-lsp front-end over `vilan-core`.
//! Analyzes each open document on change and answers diagnostics, hover,
//! go-to-definition, find-references, and rename — across files into `std`.

mod document;
mod line_index;
mod manifest_completion;
mod publish;
mod uri;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use tower_lsp::jsonrpc::{Error as JsonRpcError, ErrorCode};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server, jsonrpc::Result};
use vilan_core::Span;
use vilan_core::analyzer::SourceId;

use crate::document::{
    Completion, CompletionKind as VilanCompletionKind, Document, Symbol,
    SymbolKind as VilanSymbolKind, hash_text,
};
use crate::line_index::LineIndex;
use crate::publish::PublishState;

/// How long to wait after the last edit before re-analyzing, so a burst of
/// keystrokes collapses to a single analysis instead of one per character.
const DEBOUNCE_MS: u64 = 150;

/// How completion inserts a function or method call — the `vilan.completion.functionCall`
/// setting, consumed by [`to_completion_item`]: `Full` fills named parameter
/// tab-stops, `ParensOnly` inserts the parentheses, `None` inserts the bare name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CompletionFunctionCall {
    /// Insert the name only.
    None,
    /// Insert `name()` (empty parentheses).
    ParensOnly,
    /// Insert `name(…)` with a placeholder argument list.
    Full,
}

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
/// the setting is `none` — leaving the bare label. `full` fills each parameter
/// as a named tab-stop (`name(${1:a}, ${2:b})$0`); `parensOnly` positions the
/// cursor between the parens (`name($0)`); both write `name()$0` for a
/// zero-parameter callable. Without client snippet support every shape degrades
/// to the plain `name()` (cursor after) — a snippet's tab-stops would otherwise
/// surface as literal text.
fn call_insertion(
    label: &str,
    parameters: &[String],
    mode: CompletionFunctionCall,
    snippet_support: bool,
) -> Option<(String, InsertTextFormat)> {
    if matches!(mode, CompletionFunctionCall::None) {
        return None;
    }
    if !snippet_support {
        return Some((format!("{label}()"), InsertTextFormat::PLAIN_TEXT));
    }
    let snippet = if parameters.is_empty() {
        format!("{label}()$0")
    } else {
        match mode {
            CompletionFunctionCall::Full => {
                let placeholders: Vec<String> = parameters
                    .iter()
                    .enumerate()
                    .map(|(index, name)| format!("${{{}:{name}}}", index + 1))
                    .collect();
                format!("{label}({})$0", placeholders.join(", "))
            }
            // `parensOnly` (with parameters): cursor inside the parens.
            _ => format!("{label}($0)"),
        }
    };
    Some((snippet, InsertTextFormat::SNIPPET))
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
    publish_state: Arc<std::sync::Mutex<PublishState>>,
    /// `std` files don't change during a session, so cache their line indices
    /// rather than re-reading the file on every cross-file definition/reference.
    line_indices: Arc<DashMap<PathBuf, Arc<LineIndex>>>,
    /// The client's feature settings, seeded from `initializationOptions` and
    /// updated live by `workspace/didChangeConfiguration`. Read per request
    /// (`inlay_hint`, `semantic_tokens_full`, …) so a toggle takes effect without
    /// re-registering capabilities.
    config: Arc<std::sync::RwLock<Config>>,
    /// Whether the client can render snippet completions (`$1`/`${1:name}`
    /// tab-stops). Captured from `ClientCapabilities` at `initialize` (fixed for
    /// the session); when absent, call-shaped completions degrade to plain text
    /// (WO-3).
    snippet_support: Arc<AtomicBool>,
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
    use crate::document::{AutoImport, Completion, CompletionKind, SnippetInsertion};
    use crate::line_index::LineIndex;
    use tower_lsp::lsp_types::{CompletionItemKind, Documentation, InsertTextFormat};

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
        // A document with no analysis yet (never possible today — `did_open`
        // analyzes inline — but the skip must not swallow the work if it ever is).
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
    uri: Url,
    text: String,
) -> bool {
    let path = uri.to_file_path().unwrap_or_default();
    let std_dir = discover_std_dir(&path);
    let analysis = match tokio::task::spawn_blocking(move || {
        Document::analyze(&text, &std_dir, &path)
    })
    .await
    {
        Ok(analysis) => analysis,
        Err(_) => return false,
    };
    if !land(documents, &uri, analysis) {
        return false;
    }
    publish_document(documents, client, publish_state, &uri).await;
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
    uri: &Url,
) {
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
async fn reanalyze_dependents(
    documents: &DashMap<Url, Document>,
    client: &Client,
    publish_state: &std::sync::Mutex<PublishState>,
    changed: &Url,
) -> bool {
    let changed_path = changed.to_file_path().ok();
    let dependents: Vec<(Url, String)> = documents
        .iter()
        .filter(|entry| entry.key() != changed)
        .filter(|entry| match &changed_path {
            Some(path) => entry.value().depends_on(path),
            None => true,
        })
        .map(|entry| (entry.key().clone(), entry.value().text.clone()))
        .collect();
    let mut landed = false;
    for (uri, text) in dependents {
        landed |= analyze_and_publish(documents, client, publish_state, uri, text).await;
    }
    landed
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
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(test)]
            panic_fence_tests::maybe_inject(request);
            work()
        }));
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
            let landed =
                analyze_and_publish(&documents, &client, &publish_state, uri.clone(), text).await;
            // The edit may change what other open files see (they import this
            // one, or a file it re-exports) — bring their diagnostics up to date.
            let dependents_landed =
                reanalyze_dependents(&documents, &client, &publish_state, &uri).await;
            // The analyzed snapshot moved under the client's highlighting and
            // hints; ask for them again (S5). Every guard is long dropped here.
            send_refreshes(&client, refresh_plan(landed || dependents_landed)).await;
        });
    }

    /// The line index for a file another source's span points into, cached by
    /// path so a cross-file query doesn't re-read and re-index on every lookup.
    ///
    /// The cache holds only files whose text is STABLE for the session — which
    /// is what "on disk, not open in the editor" means. A path with a buffer
    /// registered is indexed fresh every time and never stored: its text is one
    /// keystroke old, so a stored index would misplace every range it converts
    /// from the next edit onward. (The session cache has no invalidation, by
    /// design — it was written for `std`, whose files genuinely do not change.
    /// Once `read_source` began answering from the overlay, "never invalidate"
    /// stopped being safe for anything else, so the fix is to not cache those.)
    fn line_index_for(&self, path: &Path) -> Option<Arc<LineIndex>> {
        let buffered = vilan_core::analyzer::document_overlay_contains(path);
        if !buffered && let Some(cached) = self.line_indices.get(path) {
            return Some(Arc::clone(cached.value()));
        }
        // A disk read is BOM-stripped, matching the analyzer's read of the same
        // file (windows-support.md §2); a buffer comes back verbatim. Either
        // way this index and the spans it converts index the same text the
        // analyzer saw, which is the whole point.
        let text = vilan_core::util::read_source(path).ok()?;
        let line_index = Arc::new(LineIndex::new(&text));
        if !buffered {
            self.line_indices
                .insert(path.to_path_buf(), Arc::clone(&line_index));
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
                capabilities: ServerCapabilities {
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
                    linked_editing_range_provider: Some(
                        LinkedEditingRangeServerCapabilities::Simple(true),
                    ),
                    definition_provider: Some(OneOf::Left(true)),
                    references_provider: Some(OneOf::Left(true)),
                    rename_provider: Some(OneOf::Left(true)),
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
                    semantic_tokens_provider: Some(
                        SemanticTokensServerCapabilities::SemanticTokensOptions(
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
                        ),
                    ),
                    // WO-2: the "Organize Imports" source action (sort + prune).
                    // E54: QUICKFIX (add-import, and E58's field-name rename) and
                    // the "add all missing imports" source action.
                    code_action_provider: Some(CodeActionProviderCapability::Options(
                        CodeActionOptions {
                            code_action_kinds: Some(vec![
                                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                                CodeActionKind::QUICKFIX,
                                fix_all_imports_kind(),
                            ]),
                            ..Default::default()
                        },
                    )),
                    ..Default::default()
                },
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
        // Analyze inline and insert the document before the first `.await`, so a
        // query that arrives right after open — before diagnostics are published
        // — still finds it. (The debounced change path runs off the async thread,
        // but there a previous analysis is always already in place.)
        let uri = params.text_document.uri;
        // The synchronous prefix fences (B40); the trailing publish is pure
        // message sending. A panicked open publishes nothing — the map entry
        // it failed to make is what "open" means everywhere else.
        let publish = self.fenced("didOpen", false, || {
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
                return false;
            }
            let path = uri.to_file_path().unwrap_or_default();
            // Register the buffer so OTHER documents' analyses load this one's
            // live content instead of the file on disk (backlog E6).
            vilan_core::analyzer::set_document_overlay(
                &path,
                Some(params.text_document.text.clone()),
            );
            let std_dir = discover_std_dir(&path);
            let document = Document::analyze(&params.text_document.text, &std_dir, &path);
            // The ONLY place a document enters the map. Every later analysis lands
            // by merge onto what is here (`land`), which is what lets a result
            // arriving after `did_close` be dropped instead of resurrecting the
            // file: a missing entry can only mean "closed", never "not opened yet".
            self.documents.insert(uri.clone(), document);
            true
        });
        if publish {
            publish_document(&self.documents, &self.client, &self.publish_state, &uri).await;
        }
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
            self.on_change(uri, text);
        })
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // A save changes what OTHER documents' analyses read from disk (module
        // loading is disk-backed), so re-analyze every open document.
        let saved = params.text_document.uri;
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
                uri,
                text,
            )
            .await;
        }
        landed |=
            reanalyze_dependents(&self.documents, &self.client, &self.publish_state, &saved).await;
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
        self.documents.remove(&uri);
        self.semantic_token_cache.remove(&uri);
        // Drop the edit generation so any in-flight debounced analysis bails.
        self.pending.remove(&uri);
        // Clear this document's diagnostics AND the ones it published onto
        // other files — each target republishes as the remaining owners'
        // merged view (empty where this was the only contributor).
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
                .inlay_hints()
                .into_iter()
                .filter_map(|(offset, label)| {
                    // The anchor is a program offset, so it converts through the
                    // ANALYZED index (S1). Through the live one, an insertion above
                    // slid every hint below it — and the viewport filter on the next
                    // line then dropped the ones that slid out of range entirely.
                    //
                    // The filter compares against `params.range`, which is
                    // live-space. With incremental sync (B39c) the recorded
                    // edits map the anchor into live space and the compare
                    // is EXACT; when the map is broken (a whole-text set, an
                    // analysis of an older text) it falls back to the old
                    // approximation — exact for same-line edits, off by the
                    // inserted or deleted lines near the viewport edge until
                    // the refresh lands. The HINT keeps its analyzed-space
                    // position either way: program answers describe the
                    // analyzed snapshot (the snapshot-consistency rule), and
                    // the client clips out-of-range answers harmlessly.
                    let position = document.analyzed_position(offset);
                    let visible = match document.live_offset(offset) {
                        Some(live) => {
                            let live_position = document.line_index.position(live);
                            live_position >= range.start && live_position <= range.end
                        }
                        None => position >= range.start && position <= range.end,
                    };
                    visible.then(|| InlayHint {
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
            let data =
                encode_semantic_tokens(&document.semantic_tokens(), document.analyzed_index());
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
            let data =
                encode_semantic_tokens(&document.semantic_tokens(), document.analyzed_index());
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
            // Filter the ABSOLUTE tokens to the requested lines, then encode:
            // the first kept token's delta is from the document start, which
            // is exactly the encoding a range response specifies. Line
            // granularity is what editors ask with (a viewport), and a token
            // never spans lines (the encoder drops any that would).
            let index = document.analyzed_index();
            let start_line = params.range.start.line;
            let end_line = params.range.end.line;
            let tokens: Vec<_> = document
                .semantic_tokens()
                .into_iter()
                .filter(|(span, _, _)| {
                    let line = index.range(span).start.line;
                    line >= start_line && line <= end_line
                })
                .collect();
            let data = encode_semantic_tokens(&tokens, index);
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
            let items = document
                .completion(offset)
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
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            let offset = document.analyzed_offset(position);
            let locations = document
                .references(offset)
                .into_iter()
                .filter_map(|(source, span)| self.location_for(&document, &uri, source, span))
                .collect();
            Ok(Some(locations))
        })
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        self.fenced("rename", Err(handler_panicked()), || {
            let uri = params.text_document_position.text_document.uri;
            let position = params.text_document_position.position;
            let new_name = params.new_name;
            let Some(document) = self.documents.get(&uri) else {
                return Ok(None);
            };
            // S3: a rename is edits computed from program data. Applying them to a
            // buffer that has moved on corrupts it, so refuse while the snapshots
            // diverge instead of guessing. At human timescales this is invisible —
            // a rename happens at rest, after the debounce has landed.
            if document.is_stale() {
                return Err(still_analyzing());
            }
            let offset = document.analyzed_offset(position);
            let occurrences = document.references(offset);
            if occurrences.is_empty() {
                return Ok(None);
            }
            let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
            for (source, span) in occurrences {
                if let Some(location) = self.location_for(&document, &uri, source, span) {
                    changes.entry(location.uri).or_default().push(TextEdit {
                        range: location.range,
                        new_text: new_name.clone(),
                    });
                }
            }
            Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }))
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
            // Skip the work entirely when the client asked for a kind none of
            // these three answer.
            if !wants_organize && !wants_quickfix && !wants_fix_all_imports {
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
                if wants_fix_all_imports {
                    if let Some((span, new_text)) = document.add_all_missing_imports_edit(program) {
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
    // request fail mid-typing.
    #[tokio::test]
    async fn a_stale_document_still_answers_an_unoffered_code_action_kind() {
        let (service, _socket) = backend();
        let backend = service.inner();
        let uri = open_with_live_edit(backend, EDITED);
        let mut params = code_action_params(&uri);
        params.context.only = Some(vec![CodeActionKind::REFACTOR]);
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

    // S1/S3: read-only queries never refuse — they answer
    // correctly-for-the-snapshot. Semantic tokens over a stale buffer come back
    // byte-identical to the pre-edit answer, which is what stops the
    // highlighting from breaking up while the analysis catches up.
    #[tokio::test]
    async fn semantic_tokens_answer_the_analyzed_snapshot_while_typing() {
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
        // The DATA holds still; the `result_id` is fresh per response by
        // design (B39b's delta chain), so the comparison names the claim.
        let data_of = |answer: Option<SemanticTokensResult>| match answer {
            Some(SemanticTokensResult::Tokens(tokens)) => tokens.data,
            other => panic!("the full provider returns tokens, got {other:?}"),
        };
        let baseline = data_of(baseline);
        assert_eq!(
            baseline,
            data_of(mid_edit),
            "the answer holds still until the analysis lands",
        );
        assert!(!baseline.is_empty(), "the fixture must produce tokens");
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
