//! E263: B495's mode-mismatch refusal — a value closure where its type takes a
//! view, or the reverse — carries a quick fix: the literal's parameter written
//! in the type's mode where the literal is right there, the adapter the
//! refusal names around a named closure otherwise. The edit's data is
//! `vilan_ide::closure_mode_fix`; this is the code action over the real
//! refusal, so a reworded message reds here.

use std::path::Path;

use crate::document::tests::std_root;
use crate::document::{Document, QuickFix};

fn offered(document: &Document) -> Vec<QuickFix> {
    let program = document.program.as_ref().expect("a program");
    document
        .diagnostics
        .iter()
        .flat_map(|diagnostic| document.quickfixes(program, diagnostic.span))
        .collect()
}

/// `source` with the ONE fix titled `title` applied, and checked clean.
fn fixed_clean(source: &str, title: &str) -> String {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    let fixes: Vec<QuickFix> = offered(&document)
        .into_iter()
        .filter(|fix| fix.title == title)
        .collect();
    assert_eq!(
        fixes.len(),
        1,
        "exactly one `{title}` fix for {source:?}: {:?} {:?}",
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
    let after = Document::analyze(&text, &std_root(), Path::new("test.vl"));
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

const APPLY: &str = "fun apply(f: |&str| i32): i32 {\n\tf(&\"hi\")\n}\n\n";

#[test]
fn a_literal_argument_takes_the_view() {
    let after = fixed_clean(
        &format!("{APPLY}fun main() {{\n\tprint(apply(|s: str| 1));\n}}\n"),
        "Take the parameter as `&str`",
    );
    assert!(after.contains("apply(|s: &str| 1)"), "{after}");
}

#[test]
fn an_annotated_binding_takes_the_value() {
    let after = fixed_clean(
        "fun main() {\n\tlet by_value: |str| void = |c: &str| {};\n\tby_value(\"x\");\n}\n",
        "Take the parameter as `str`",
    );
    assert!(after.contains("= |c: str| {}"), "{after}");
}

#[test]
fn a_writable_view_parameter_is_written_mut() {
    let after = fixed_clean(
        "fun apply(f: |&mut i32| void, n: &mut i32) {\n\tf(n);\n}\n\nfun main() {\n\tmut n = 1;\n\tapply(|x: i32| {}, &mut n);\n\tprint(n);\n}\n",
        "Take the parameter as `&mut i32`",
    );
    assert!(after.contains("apply(|x: &mut i32| {}, &mut n)"), "{after}");
}

#[test]
fn a_named_value_closure_is_adapted() {
    let after = fixed_clean(
        &format!(
            "{APPLY}fun main() {{\n\tlet g = |s: str| s.len().as_i32();\n\tprint(apply(g));\n}}\n"
        ),
        "Adapt it: `|c| g(*c)`",
    );
    assert!(after.contains("apply(|c| g(*c))"), "{after}");
}
