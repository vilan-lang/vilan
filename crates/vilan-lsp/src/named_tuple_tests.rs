//! B569 (`named-tuple-fields.md`): what the editor shows of a tuple's labels —
//! hover prints them (§5) — and the two quick fixes the label contradiction
//! carries (§4.3, RULED). The fix data is `vilan_ide::tuple_label_fix`; this is
//! the code action over the real refusal, so a reworded message reds here.

use std::path::Path;

use crate::document::tests::std_root;
use crate::document::{Document, QuickFix};

fn analyzed(source: &str) -> Document {
    Document::analyze(source, &std_root(), Path::new("test.vl"))
}

fn offered(document: &Document) -> Vec<QuickFix> {
    let program = document.program.as_ref().expect("a program");
    document
        .diagnostics
        .iter()
        .flat_map(|diagnostic| document.quickfixes(program, diagnostic.span))
        .collect()
}

const SWAP: &str = "fun main() {\n\tlet p: (x: f64, y: f64) = (x = 5, y = 7);\n\tlet q: (y: f64, x: f64) = p;\n\tprint(q.y);\n}\n";

/// The source with the fix titled `title` applied, checked clean.
fn fixed_clean(source: &str, title: &str) -> String {
    let document = analyzed(source);
    let fixes: Vec<QuickFix> = offered(&document)
        .into_iter()
        .filter(|fix| fix.title == title)
        .collect();
    assert_eq!(
        fixes.len(),
        1,
        "exactly one `{title}` fix: {:?} {:?}",
        offered(&document)
            .iter()
            .map(|fix| fix.title.clone())
            .collect::<Vec<_>>(),
        document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
    let mut text = source.to_string();
    text.replace_range(fixes[0].span.into_range(), &fixes[0].replacement);
    let after = analyzed(&text);
    assert!(
        after.diagnostics.is_empty(),
        "the fixed program checks clean: {text:?} {:?}",
        after
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
    text
}

#[test]
fn b569_a_reordered_label_set_is_fixed_by_name() {
    let after = fixed_clean(SWAP, "Match by name: `(y = p.y, x = p.x)`");
    assert!(
        after.contains("let q: (y: f64, x: f64) = (y = p.y, x = p.x);"),
        "{after}"
    );
}

#[test]
fn b569_a_reordered_label_set_is_fixed_by_position() {
    let after = fixed_clean(SWAP, "Match by position: `(p.0, p.1)`");
    assert!(
        after.contains("let q: (y: f64, x: f64) = (p.0, p.1);"),
        "{after}"
    );
}

/// A renamed slot (`(x, y)` into `(y, z)`) has no `z` to read by name, so the
/// position is the one fix.
#[test]
fn b569_a_renamed_label_offers_the_position_alone() {
    let source = "fun main() {\n\tlet p: (x: f64, y: f64) = (x = 5, y = 7);\n\tlet q: (y: f64, z: f64) = p;\n\tprint(q.z);\n}\n";
    let titles: Vec<String> = offered(&analyzed(source))
        .into_iter()
        .map(|fix| fix.title)
        .collect();
    assert_eq!(titles, vec!["Match by position: `(p.0, p.1)`".to_string()]);
}

#[test]
fn b569_hover_prints_a_tuples_labels() {
    let source = "fun main() {\n\tlet point = (x = 5.0, y = 7.0);\n\tprint(point.x);\n}\n";
    let offset = source.find("point =").expect("the binding");
    let hover = analyzed(source).hover(offset).expect("a hover");
    assert!(hover.contains("point: (x: f64, y: f64)"), "{hover}");
}

#[test]
fn b569_hover_on_a_function_prints_its_labelled_return() {
    let source = "fun bounds(): (min: i32, max: i32) {\n\t(max = 9, min = 1)\n}\n\nfun main() {\n\tprint(bounds().max);\n}\n";
    let offset = source.find("bounds").expect("the function");
    let hover = analyzed(source).hover(offset).expect("a hover");
    assert!(hover.contains("(min: i32, max: i32)"), "{hover}");
}
