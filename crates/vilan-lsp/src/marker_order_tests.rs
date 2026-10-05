//! B536's editor half: a declaration head written out of THE order publishes
//! with its stable code (`marker-order/attributes` for the warning on
//! attributes out of rank, `marker-order/keywords` for the refusal of a keyword
//! ahead of an attribute or two keywords inverted) and carries a quick fix that
//! writes the head in the order — one head at a time, and every head in the
//! file at once. The diagnostics and the edit's data are
//! `vilan_core::parsing::{MarkerOrderDiagnostic, marker_order_fix}`; this is
//! the code action over them. R-c: the attribute-order warning becomes an
//! error in v0.45.0, and this fix is the migration the flip waits on.

use std::path::Path;

use tower_lsp::lsp_types::{NumberOrString, Url};

use crate::document::tests::std_root;
use crate::document::{Document, QuickFix};
use crate::publish::PublishState;

/// Every quick fix offered over each of `document`'s diagnostics and
/// warnings, asked at the finding's own span (as a client asks at the caret).
fn offered(document: &Document) -> Vec<QuickFix> {
    let program = document.program.as_ref().expect("a program");
    document
        .diagnostics
        .iter()
        .chain(&document.warnings)
        .flat_map(|finding| document.quickfixes(program, finding.span))
        .collect()
}

/// `source` with the ONE fix titled `title` applied.
fn fixed(source: &str, title: &str) -> String {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    let mut fixes: Vec<(vilan_core::Span, String)> = offered(&document)
        .into_iter()
        .filter(|fix| fix.title == title)
        .map(|fix| (fix.span, fix.replacement))
        .collect();
    fixes.dedup();
    assert_eq!(
        fixes.len(),
        1,
        "exactly one `{title}` fix for {source:?}: {:?}",
        offered(&document)
            .iter()
            .map(|fix| fix.title.clone())
            .collect::<Vec<_>>()
    );
    let (span, replacement) = fixes.remove(0);
    let mut text = source.to_string();
    text.replace_range(span.into_range(), &replacement);
    text
}

/// The fixed program carries neither an error nor a warning.
fn clean(source: &str) {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    assert!(
        document.diagnostics.is_empty() && document.warnings.is_empty(),
        "the fixed program checks clean: {source:?} {:?}",
        document
            .diagnostics
            .iter()
            .chain(&document.warnings)
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn attributes_out_of_rank_are_reordered() {
    let after = fixed(
        "[must_use] [deprecated(\"d\")] fun one(): i32 { 1 }\n\nfun main() {}\n",
        "Write `[deprecated(..)] [must_use] fun`",
    );
    assert_eq!(
        after,
        "[deprecated(\"d\")] [must_use] fun one(): i32 { 1 }\n\nfun main() {}\n"
    );
    clean(&after);
}

/// Attributes stacked on their own lines, with a comment between them: the
/// units move and the lines, the comment and the indentation stay.
#[test]
fn a_stacked_head_keeps_its_lines_and_comments() {
    let after = fixed(
        "[must_use]\n// why\n[deprecated(\"d\")]\nfun one(): i32 { 1 }\n\nfun main() {}\n",
        "Write `[deprecated(..)] [must_use] fun`",
    );
    assert_eq!(
        after,
        "[deprecated(\"d\")]\n// why\n[must_use]\nfun one(): i32 { 1 }\n\nfun main() {}\n"
    );
    clean(&after);
}

/// The refusal's half: `export` ahead of an attribute (B485 S3) is an error,
/// and the same fix writes it in the order.
#[test]
fn a_keyword_ahead_of_an_attribute_is_reordered() {
    let after = fixed(
        "export [must_use] fun one(): i32 { 1 }\n\nfun main() {}\n",
        "Write `[must_use] export fun`",
    );
    assert_eq!(
        after,
        "[must_use] export fun one(): i32 { 1 }\n\nfun main() {}\n"
    );
    clean(&after);
}

/// Both at once: keywords inverted AND attributes out of rank are one head,
/// one diagnostic and one edit.
#[test]
fn a_head_wrong_both_ways_is_one_edit() {
    let after = fixed(
        "external async [platform(\"node\")] [deprecated(\"d\")] fun now(): i32;\n\nfun main() {}\n",
        "Write `[deprecated(..)] [platform(..)] async external fun`",
    );
    assert_eq!(
        after,
        "[deprecated(\"d\")] [platform(\"node\")] async external fun now(): i32;\n\nfun main() {}\n"
    );
}

/// The file-wide fix: every head in the file — warnings and refusals alike —
/// in ONE edit, offered from any one of them, the text between the heads
/// carried through untouched.
#[test]
fn every_head_in_the_file_is_reordered_at_once() {
    let source = concat!(
        "[must_use] [deprecated(\"a\")] fun one(): i32 { 1 }\n\n",
        "export [must_use] fun two(): i32 { 2 }\n\n",
        "// between\n",
        "[internal(\"r\")] [derive(Clone)] struct Point {\n\tx: i32,\n}\n\n",
        "fun main() {}\n"
    );
    let title = "Write all 3 declaration heads in the order";
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    let findings: Vec<_> = document
        .diagnostics
        .iter()
        .chain(&document.warnings)
        .filter(|finding| {
            vilan_core::parsing::MarkerOrderDiagnostic::of_message(&finding.msg).is_some()
        })
        .collect();
    assert_eq!(findings.len(), 3, "the premise: three heads out of order");
    for finding in &findings {
        let titles: Vec<String> = document
            .quickfixes(program, finding.span)
            .into_iter()
            .map(|fix| fix.title)
            .collect();
        assert!(
            titles.iter().any(|offered| offered == title),
            "the file-wide fix is offered at {:?}: {titles:?}",
            finding.span
        );
    }
    let after = fixed(source, title);
    assert_eq!(
        after,
        concat!(
            "[deprecated(\"a\")] [must_use] fun one(): i32 { 1 }\n\n",
            "[must_use] export fun two(): i32 { 2 }\n\n",
            "// between\n",
            "[derive(Clone)] [internal(\"r\")] struct Point {\n\tx: i32,\n}\n\n",
            "fun main() {}\n"
        )
    );
}

/// With one head out of order the file-wide fix would be the per-head fix
/// under a longer name: it is not offered.
#[test]
fn one_head_offers_no_file_wide_fix() {
    let document = Document::analyze(
        "[must_use] [deprecated(\"a\")] fun one(): i32 { 1 }\n\nfun main() {}\n",
        &std_root(),
        Path::new("test.vl"),
    );
    let titles: Vec<String> = offered(&document)
        .into_iter()
        .map(|fix| fix.title)
        .collect();
    assert!(
        titles.iter().all(|title| !title.starts_with("Write all ")),
        "{titles:?}"
    );
}

/// Both diagnostics are published with their stable codes, which a client
/// (and the fix) can key on whatever the messages' wording becomes.
#[test]
fn the_diagnostics_are_published_with_their_codes() {
    let path = std::env::temp_dir().join(format!(
        "vilan-b536-code-{}-{:?}.vl",
        std::process::id(),
        std::thread::current().id()
    ));
    let document = Document::analyze(
        "[must_use] [deprecated(\"a\")] fun one(): i32 { 1 }\n\nexport [must_use] fun two(): i32 { 2 }\n\nfun main() {}\n",
        &std_root(),
        &path,
    );
    let uri = Url::from_file_path(&path).expect("a file url");
    let actions = PublishState::new().plan_publish(&uri, &document);
    let mut codes: Vec<Option<NumberOrString>> = actions
        .iter()
        .find(|(target, _)| *target == uri)
        .map(|(_, group)| {
            group
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect()
        })
        .unwrap_or_default();
    codes.sort_by_key(|code| format!("{code:?}"));
    assert_eq!(
        codes,
        vec![
            Some(NumberOrString::String(
                "marker-order/attributes".to_string()
            )),
            Some(NumberOrString::String("marker-order/keywords".to_string())),
        ]
    );
}
