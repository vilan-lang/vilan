//! A157's editor half: a written `autofocus` attribute in an element head
//! (`<input autofocus />`) publishes with its stable code
//! (`element-attribute/autofocus`) and carries a quick fix that rewrites it to
//! `.autofocus()` in place. The analyzer raises the steer and
//! `vilan_core::elements::written_autofocus_fix` is the fix's data; this is
//! the code action over it, in B520's shape (`foreign_spelling_tests.rs`).

use std::path::Path;

use tower_lsp::lsp_types::{NumberOrString, Url};

use crate::document::Document;
use crate::document::tests::std_root;
use crate::publish::PublishState;

const TITLE: &str = "Write `.autofocus()`";

#[test]
fn a_written_autofocus_becomes_the_method() {
    let source = "import std::ui::View;\n\nfun main() {\n\tlet _element: View = <input name(\"n\") autofocus />;\n}\n";
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    // A warning is not in `diagnostics` (document.rs says why): the fix is
    // asked for over the warning's own span.
    let fixes: Vec<_> = document
        .warnings
        .iter()
        .flat_map(|warning| document.quickfixes(program, warning.span))
        .filter(|fix| fix.title == TITLE)
        .collect();
    assert_eq!(fixes.len(), 1, "one `{TITLE}` fix for {source:?}");
    let mut text = source.to_string();
    text.replace_range(fixes[0].span.into_range(), &fixes[0].replacement);
    assert_eq!(
        text,
        "import std::ui::View;\n\nfun main() {\n\tlet _element: View = <input name(\"n\") .autofocus() />;\n}\n"
    );
    let fixed = Document::analyze(&text, &std_root(), Path::new("test.vl"));
    assert!(
        fixed.diagnostics.is_empty() && fixed.warnings.is_empty(),
        "the fixed program checks clean: {:?}",
        fixed
            .diagnostics
            .iter()
            .chain(&fixed.warnings)
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
}

/// The diagnostic is published with its stable code, which a client (and the
/// fix) can key on whatever the message's wording becomes.
#[test]
fn the_steer_is_published_with_its_code() {
    let path = std::env::temp_dir().join(format!("vilan-a157-code-{}.vl", std::process::id()));
    let document = Document::analyze(
        "import std::ui::View;\n\nfun main() {\n\tlet _element: View = <input autofocus />;\n}\n",
        &std_root(),
        &path,
    );
    let uri = Url::from_file_path(&path).expect("a file url");
    let actions = PublishState::new().plan_publish(&uri, &document);
    let codes: Vec<Option<NumberOrString>> = actions
        .iter()
        .find(|(target, _)| *target == uri)
        .map(|(_, group)| {
            group
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        codes,
        vec![Some(NumberOrString::String(
            "element-attribute/autofocus".to_string()
        ))]
    );
}
