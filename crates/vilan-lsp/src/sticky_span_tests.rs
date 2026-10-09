//! E279: spans stick to their text across edits. Between a `didChange` and the
//! analysis it schedules, every answer the server gives about the buffer —
//! the published diagnostics (E242), and now the caret requests (hover,
//! go-to-definition, find-references), the outline and the quick fixes — is
//! the last analysis's, carried through the edits since onto the text on
//! screen.
//!
//! The pins drive the real handlers. The ordering argument for "no analysis
//! ran" is the harness's own, not a sleep: `did_change` schedules its analysis
//! behind a `DEBOUNCE_MS` timer on a spawned task, `#[tokio::test]` runs on a
//! current-thread runtime, and every assertion below reads the server's state
//! without yielding — so the spawned task has not even been polled when the
//! counters are read, and they say so.

use std::path::Path;

use tower_lsp::LanguageServer;
use tower_lsp::lsp_types::{
    CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    DidChangeTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, HoverParams, Location, Position, Range,
    ReferenceContext, ReferenceParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier,
};

use crate::Backend;
use crate::document::Document;
use crate::document::tests::std_root;
use crate::snapshot_consistency_tests::backend;

fn uri() -> Url {
    Url::parse("file:///e279/main.vl").expect("a url")
}

/// `source` analyzed and opened at [`uri`], its diagnostics published into the
/// planner exactly as an analysis landing publishes them.
fn open_published(backend: &Backend, source: &str) -> Url {
    let uri = uri();
    let document = Document::analyze(source, &std_root(), Path::new("/e279/main.vl"));
    backend.documents.insert(uri.clone(), document);
    let document = backend.documents.get(&uri).expect("open");
    backend
        .publish_state
        .lock()
        .expect("the planner")
        .plan_publish(&uri, &document);
    uri
}

/// One ranged content change — `text` replacing `start..end` (positions in
/// the text as it stands before this change).
fn change(start: Position, end: Position, text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: Some(Range::new(start, end)),
        range_length: None,
        text: text.to_string(),
    }
}

async fn type_into(backend: &Backend, uri: &Url, changes: Vec<TextDocumentContentChangeEvent>) {
    backend
        .did_change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: changes,
        })
        .await;
}

/// What the server would put on the wire for `uri` right now — the action
/// `did_change` sends after the edit (`PublishState::republish`).
fn published(backend: &Backend, uri: &Url) -> Vec<Range> {
    let (_, diagnostics) = backend
        .publish_state
        .lock()
        .expect("the planner")
        .republish(uri);
    diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.range)
        .collect()
}

/// No analysis has started, and the document still holds the analysis of
/// `analyzed` — the answers under test can only have come from the shifted
/// set.
fn assert_no_analysis_ran(backend: &Backend, uri: &Url, analyzed: &str) {
    let counts = backend.analyses.counts();
    assert_eq!(
        (counts.started, counts.landed),
        (0, 0),
        "no analysis may have run"
    );
    let document = backend.documents.get(uri).expect("open");
    assert_eq!(
        document.analyzed_text(),
        analyzed,
        "the analysis is the old one"
    );
    assert_ne!(document.text, analyzed, "the buffer is ahead of it");
}

const DIAGNOSED: &str =
    "fun main() {\n\tlet ok = 1;\n\tlet wrong: i32 = \"text\";\n\tlet after = ok;\n}\n\nmain();\n";

/// The one published range, on line 2 of [`DIAGNOSED`] (the string literal).
fn the_diagnostic(backend: &Backend, uri: &Url) -> Range {
    let ranges = published(backend, uri);
    assert_eq!(
        ranges.len(),
        1,
        "the fixture publishes one diagnostic: {ranges:?}"
    );
    ranges[0]
}

// --- Diagnostics: the published range moves before any analysis runs -------

#[tokio::test]
async fn a_line_typed_above_a_diagnostic_moves_its_published_range_before_any_analysis() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, DIAGNOSED);
    let before = the_diagnostic(backend, &uri);
    assert_eq!(before.start.line, 2, "the fixture's error is on line 2");
    let note = "\t// a note\n";
    type_into(
        backend,
        &uri,
        vec![change(Position::new(1, 0), Position::new(1, 0), note)],
    )
    .await;
    assert_no_analysis_ran(backend, &uri, DIAGNOSED);
    let after = the_diagnostic(backend, &uri);
    assert_eq!(
        after,
        Range::new(
            Position::new(before.start.line + 1, before.start.character),
            Position::new(before.end.line + 1, before.end.character),
        ),
        "the squiggle moved down the one line typed above it",
    );
}

#[tokio::test]
async fn text_typed_before_a_diagnostic_on_its_line_moves_it_along_the_line() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, DIAGNOSED);
    let before = the_diagnostic(backend, &uri);
    // `let wrong` -> `let  wrong`: three bytes in, before the literal.
    type_into(
        backend,
        &uri,
        vec![change(Position::new(2, 4), Position::new(2, 4), " ")],
    )
    .await;
    assert_no_analysis_ran(backend, &uri, DIAGNOSED);
    let after = the_diagnostic(backend, &uri);
    assert_eq!(after.start.line, before.start.line);
    assert_eq!(after.start.character, before.start.character + 1);
    assert_eq!(after.end.character, before.end.character + 1);
}

#[tokio::test]
async fn an_edit_inside_a_diagnostic_grows_or_truncates_it() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, DIAGNOSED);
    let before = the_diagnostic(backend, &uri);
    assert!(
        before.end.character - before.start.character >= 4,
        "the fixture's range spans the literal: {before:?}"
    );
    let inside = Position::new(2, before.start.character + 2);
    // Grows: two characters typed inside the flagged literal.
    type_into(backend, &uri, vec![change(inside, inside, "ab")]).await;
    let grown = the_diagnostic(backend, &uri);
    assert_eq!(grown.start, before.start, "the start stays on its text");
    assert_eq!(
        grown.end.character,
        before.end.character + 2,
        "the end grew"
    );
    // Truncates: the same two characters and one more deleted.
    let end = Position::new(2, inside.character + 3);
    type_into(backend, &uri, vec![change(inside, end, "")]).await;
    assert_no_analysis_ran(backend, &uri, DIAGNOSED);
    let truncated = the_diagnostic(backend, &uri);
    assert_eq!(truncated.start, before.start);
    assert_eq!(
        truncated.end.character,
        before.end.character - 1,
        "the end shrank"
    );
}

#[tokio::test]
async fn an_edit_after_a_diagnostic_leaves_it() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, DIAGNOSED);
    let before = the_diagnostic(backend, &uri);
    // A new line after the flagged one, and text after the literal on its own
    // line: neither moves it.
    type_into(
        backend,
        &uri,
        vec![
            change(
                Position::new(3, 0),
                Position::new(3, 0),
                "\tlet more = 2;\n",
            ),
            change(
                Position::new(2, before.end.character + 1),
                Position::new(2, before.end.character + 1),
                " // why",
            ),
        ],
    )
    .await;
    assert_no_analysis_ran(backend, &uri, DIAGNOSED);
    assert_eq!(the_diagnostic(backend, &uri), before);
}

#[tokio::test]
async fn an_edit_across_a_diagnostics_end_takes_it_away_until_the_analysis() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, DIAGNOSED);
    let before = the_diagnostic(backend, &uri);
    let straddle = Range::new(
        Position::new(2, before.end.character - 1),
        Position::new(2, before.end.character + 1),
    );
    type_into(
        backend,
        &uri,
        vec![change(straddle.start, straddle.end, "")],
    )
    .await;
    assert_no_analysis_ran(backend, &uri, DIAGNOSED);
    assert!(published(backend, &uri).is_empty());
}

// --- Caret requests answer the shifted set ----------------------------------

const NAMED: &str =
    "fun main() {\n\tlet value = 1;\n\tlet other = \"text\";\n\tlet copy = value;\n}\n";
const ABOVE: &str = "// a new first line\n";

/// [`NAMED`] opened, with [`ABOVE`] typed at its top as a ranged edit — every
/// live line is the analyzed line plus one.
async fn named_with_a_line_above(backend: &Backend) -> Url {
    let uri = open_published(backend, NAMED);
    type_into(
        backend,
        &uri,
        vec![change(Position::new(0, 0), Position::new(0, 0), ABOVE)],
    )
    .await;
    assert_no_analysis_ran(backend, &uri, NAMED);
    uri
}

fn at(uri: &Url, line: u32, character: u32) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        position: Position::new(line, character),
    }
}

async fn hover_text(backend: &Backend, position: TextDocumentPositionParams) -> Option<String> {
    backend
        .hover(HoverParams {
            text_document_position_params: position,
            work_done_progress_params: Default::default(),
        })
        .await
        .expect("hover answers")
        .map(|hover| format!("{:?}", hover.contents))
}

#[tokio::test]
async fn hover_after_a_line_typed_above_answers_the_name_under_the_caret() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = named_with_a_line_above(backend).await;
    // `other` is on live line 3 (analyzed line 2); `value` on live line 2.
    let other = hover_text(backend, at(&uri, 3, 6)).await.expect("hovers");
    assert!(other.contains("str"), "the caret is on `other`: {other}");
    let value = hover_text(backend, at(&uri, 2, 6)).await.expect("hovers");
    assert!(value.contains("i32"), "the caret is on `value`: {value}");
}

#[tokio::test]
async fn hover_inside_text_typed_since_the_analysis_answers_nothing() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, NAMED);
    // `value` -> `valxyzue`: the caret between the typed letters names no
    // byte the analysis saw.
    type_into(
        backend,
        &uri,
        vec![change(Position::new(1, 8), Position::new(1, 8), "xyz")],
    )
    .await;
    assert_eq!(hover_text(backend, at(&uri, 1, 9)).await, None);
    // The neighbours on either side of the typed text still answer.
    let after = hover_text(backend, at(&uri, 2, 6)).await.expect("hovers");
    assert!(after.contains("str"), "{after}");
}

#[tokio::test]
async fn goto_definition_after_a_line_typed_above_lands_on_the_live_declaration() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = named_with_a_line_above(backend).await;
    // The use of `value` in `let copy = value;`: live line 4, character 12.
    let answer = backend
        .goto_definition(GotoDefinitionParams {
            text_document_position_params: at(&uri, 4, 12),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("definition answers");
    let Some(GotoDefinitionResponse::Scalar(location)) = answer else {
        panic!("one definition: {answer:?}");
    };
    assert_eq!(
        location,
        Location {
            uri: uri.clone(),
            range: Range::new(Position::new(2, 5), Position::new(2, 10)),
        },
        "`value`'s declaration, on the line it sits on now",
    );
}

#[tokio::test]
async fn references_after_a_line_typed_above_are_on_the_live_lines() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = named_with_a_line_above(backend).await;
    let mut ranges: Vec<Range> = backend
        .references(ReferenceParams {
            text_document_position: at(&uri, 2, 6),
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("references answer")
        .expect("some")
        .into_iter()
        .map(|location| location.range)
        .collect();
    ranges.sort_by_key(|range| (range.start.line, range.start.character));
    assert_eq!(
        ranges,
        vec![
            Range::new(Position::new(2, 5), Position::new(2, 10)),
            Range::new(Position::new(4, 12), Position::new(4, 17)),
        ],
        "the declaration and the use of `value`, each on its live line",
    );
}

#[tokio::test]
async fn references_follow_a_whole_text_replacement_through_the_differing_region() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, NAMED);
    // A full-sync event keeps no log: the trail is the byte-exact region.
    backend
        .documents
        .get_mut(&uri)
        .expect("open")
        .set_text(&format!("{ABOVE}{NAMED}"));
    let locations = backend
        .references(ReferenceParams {
            text_document_position: at(&uri, 4, 13),
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("references answer")
        .expect("some");
    let lines: Vec<u32> = locations
        .iter()
        .map(|location| location.range.start.line)
        .collect();
    assert!(
        lines.contains(&2) && lines.contains(&4),
        "both `value`s on their live lines: {locations:?}"
    );
}

#[tokio::test]
async fn the_outline_after_a_line_typed_above_is_on_the_live_lines() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = named_with_a_line_above(backend).await;
    let Some(DocumentSymbolResponse::Nested(symbols)) = backend
        .document_symbol(DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("symbols answer")
    else {
        panic!("a nested outline");
    };
    let main = symbols
        .iter()
        .find(|symbol| symbol.name == "main")
        .expect("`main` in the outline");
    assert_eq!(main.range.start, Position::new(1, 0));
    assert_eq!(main.range.end.line, 5, "its closing brace, one line down");
    assert_eq!(main.selection_range.start, Position::new(1, 4));
}

// --- Quick fixes are offered from the shifted set ---------------------------

const UNTERMINATED: &str = "fun main() {\n\tlet a = 1\n\tlet b = a;\n}\n\nmain();\n";

fn quickfix_params(uri: &Url, range: Range, only: Vec<CodeActionKind>) -> CodeActionParams {
    CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range,
        context: CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(only),
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    }
}

/// Every `(title, edit range, new text)` the code-action handler offers for
/// `uri` at `range`.
async fn offered_fixes(backend: &Backend, uri: &Url, range: Range) -> Vec<(String, Range, String)> {
    let actions = backend
        .code_action(quickfix_params(uri, range, vec![CodeActionKind::QUICKFIX]))
        .await
        .expect("a quick-fix request is answered while the buffer is ahead")
        .unwrap_or_default();
    actions
        .into_iter()
        .filter_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => Some(action),
            CodeActionOrCommand::Command(_) => None,
        })
        .flat_map(|action| {
            let title = action.title.clone();
            action
                .edit
                .and_then(|edit| edit.changes)
                .into_iter()
                .flat_map(|changes| changes.into_values().flatten())
                .map(move |edit| (title.clone(), edit.range, edit.new_text))
        })
        .collect()
}

fn insert_semicolon(fixes: &[(String, Range, String)]) -> Option<Range> {
    fixes
        .iter()
        .find(|(title, _, text)| title == "Insert `;`" && text == ";")
        .map(|(_, range, _)| *range)
}

#[tokio::test]
async fn a_quick_fix_after_a_line_typed_above_edits_the_live_text() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, UNTERMINATED);
    let whole = Range::new(Position::new(0, 0), Position::new(10, 0));
    let before = insert_semicolon(&offered_fixes(backend, &uri, whole).await)
        .expect("the fixture offers `Insert ;`");
    assert_eq!(before.start, Position::new(1, 10), "after `1` on line 1");
    type_into(
        backend,
        &uri,
        vec![change(Position::new(0, 0), Position::new(0, 0), "// top\n")],
    )
    .await;
    assert_no_analysis_ran(backend, &uri, UNTERMINATED);
    let after = insert_semicolon(&offered_fixes(backend, &uri, whole).await)
        .expect("still offered while the buffer is ahead");
    assert_eq!(
        after,
        Range::new(Position::new(2, 10), Position::new(2, 10))
    );
}

#[tokio::test]
async fn a_quick_fix_whose_text_was_typed_over_waits_for_the_analysis() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, UNTERMINATED);
    let whole = Range::new(Position::new(0, 0), Position::new(10, 0));
    // `1` (the byte the insertion point follows) replaced by `22`: the edit
    // reaches the fix's point, so where the `;` belongs is the analysis's to
    // say again.
    type_into(
        backend,
        &uri,
        vec![change(Position::new(1, 9), Position::new(1, 10), "22")],
    )
    .await;
    assert_eq!(
        insert_semicolon(&offered_fixes(backend, &uri, whole).await),
        None
    );
}

#[tokio::test]
async fn organize_imports_alone_still_refuses_while_the_buffer_is_ahead() {
    let (service, _socket) = backend();
    let backend = service.inner();
    let uri = open_published(backend, UNTERMINATED);
    type_into(
        backend,
        &uri,
        vec![change(Position::new(0, 0), Position::new(0, 0), "// top\n")],
    )
    .await;
    let error = backend
        .code_action(quickfix_params(
            &uri,
            Range::default(),
            vec![CodeActionKind::SOURCE_ORGANIZE_IMPORTS],
        ))
        .await
        .expect_err("a stale organize refuses");
    assert_eq!(error.code, tower_lsp::jsonrpc::ErrorCode::ContentModified);
}
