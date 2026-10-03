//! E251: Organize Imports removes DUPLICATE imports — the code action only;
//! `vilan fmt` deletes no code (the owner's rec, ruled with the item).
//!
//! The five cases as revised at Order 46's GO: identical lines collapse; a
//! name repeated in one brace group is dropped; two statements over one module
//! merge into one group; a MODULE import beside a member import of the same
//! module merges into the `self` form (`import std::reactive::{ self, draft
//! };`, `self` first; `self as name` keeps its alias); and an alias is not a
//! duplicate. Each is idempotent: organizing the result changes nothing.

use std::path::Path;

use crate::document::Document;
use crate::document::tests::std_root;

/// The buffer after Organize Imports, or the buffer itself when it offers no
/// edit.
fn organize(text: &str) -> String {
    let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
    let mut edits = document.organize_import_edits();
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.into_range().start));
    let mut organized = text.to_string();
    for (span, replacement) in edits {
        organized.replace_range(span.into_range(), &replacement);
    }
    organized
}

/// `source` organizes to `expected`, and `expected` organizes to itself.
fn assert_organizes(case: &str, source: &str, expected: &str) {
    let once = organize(source);
    assert_eq!(once, expected, "{case}");
    assert_eq!(organize(&once), once, "{case}: idempotent");
}

const BODY: &str = "\nfun main() {\n\tlet value = JsonValue::parse(\"1\");\n\tlet _ = json::parse(\"2\");\n\tlet _: Option<Json> = None;\n\tlet _ = value;\n}\n";

#[test]
fn identical_lines_collapse() {
    assert_organizes(
        "identical lines",
        &format!(
            "import std::json::{{ self, Json, JsonValue }};\nimport std::json::{{ self, Json, JsonValue }};\n{BODY}"
        ),
        &format!("import std::json::{{ self, Json, JsonValue }};\n{BODY}"),
    );
}

#[test]
fn a_repeated_name_in_one_group_is_dropped() {
    assert_organizes(
        "a repeated name",
        &format!("import std::json::{{ self, Json, JsonValue, Json }};\n{BODY}"),
        &format!("import std::json::{{ self, Json, JsonValue }};\n{BODY}"),
    );
}

#[test]
fn two_statements_over_one_module_merge() {
    assert_organizes(
        "two statements over one module",
        &format!("import std::json::Json;\nimport std::json::{{ self, JsonValue }};\n{BODY}"),
        &format!("import std::json::{{ self, Json, JsonValue }};\n{BODY}"),
    );
    assert_organizes(
        "a name reachable through two lines",
        &format!("import std::json::Json;\nimport std::json::{{ self, Json, JsonValue }};\n{BODY}"),
        &format!("import std::json::{{ self, Json, JsonValue }};\n{BODY}"),
    );
    assert_organizes(
        "two brace groups over one module",
        &format!(
            "import std::json::{{ self, Json }};\nimport std::json::{{ Json, JsonValue }};\n{BODY}"
        ),
        &format!("import std::json::{{ self, Json, JsonValue }};\n{BODY}"),
    );
}

/// The revised case (5): `import std::json; import std::json::{ Json };` is one
/// group with `self` first.
#[test]
fn a_module_import_beside_a_member_import_merges_into_the_self_form() {
    assert_organizes(
        "a module import beside its members",
        &format!("import std::json;\nimport std::json::{{ Json, JsonValue }};\n{BODY}"),
        &format!("import std::json::{{ self, Json, JsonValue }};\n{BODY}"),
    );
}

/// `self as name` keeps its alias and merges the same way.
#[test]
fn an_aliased_module_import_merges_as_self_as() {
    let body = "\nfun main() {\n\tlet value = JsonValue::parse(\"1\");\n\tlet _ = j::parse(\"2\");\n\tlet _: Option<Json> = None;\n\tlet _ = value;\n}\n";
    assert_organizes(
        "an aliased module import",
        &format!("import std::json as j;\nimport std::json::{{ Json, JsonValue }};\n{body}"),
        // `self as j` keys as the rename it is (E146: only a bare `self`
        // ranks first), so it sorts among the names.
        &format!("import std::json::{{ self as j, Json, JsonValue }};\n{body}"),
    );
}

/// Case (4): an alias is a different import, so `import a::B as D;` stays
/// beside `import a::B;`.
#[test]
fn an_alias_is_not_a_duplicate() {
    let body =
        "\nfun main() {\n\tlet _: Option<Json> = None;\n\tlet _: Option<Document> = None;\n}\n";
    let source = format!("import std::json::Json;\nimport std::json::Json as Document;\n{body}");
    assert_organizes("an alias", &source, &source);
}

/// Distinct single members of one module, with no brace group, are the
/// separate lines they always were — canonical, and not a duplicate.
#[test]
fn distinct_single_members_stay_separate() {
    let body =
        "\nfun main() {\n\tlet _: Option<Json> = None;\n\tlet _: Option<JsonValue> = None;\n}\n";
    let source = format!("import std::json::Json;\nimport std::json::JsonValue;\n{body}");
    assert_organizes("distinct members", &source, &source);
}

// --- E255: a duplicate import carries a warning, and its fix is E251's merge --

fn warnings(source: &str) -> Vec<(String, String)> {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    document
        .duplicate_import_warnings()
        .into_iter()
        .map(|(span, message)| (source[span.into_range()].to_string(), message))
        .collect()
}

#[test]
fn e255_each_repeat_carries_a_warning_naming_the_first_line() {
    let repeated = warnings(&format!(
        "import std::json::{{ self, Json, JsonValue }};\nimport std::json::Json;\n{BODY}"
    ));
    assert_eq!(
        repeated,
        vec![(
            "Json".to_string(),
            "`Json` is already imported on line 1 — Organize Imports removes the repeat"
                .to_string()
        )]
    );
    assert_eq!(
        warnings(&format!(
            "import std::json::{{ self, Json, JsonValue, Json }};\n{BODY}"
        ))
        .len(),
        1,
        "a name repeated in one group"
    );
    assert_eq!(
        warnings(&format!(
            "import std::json;\nimport std::json::{{ self, Json, JsonValue }};\n{BODY}"
        ))
        .iter()
        .map(|(leaf, _)| leaf.as_str())
        .collect::<Vec<_>>(),
        vec!["self"],
        "a `self` leaf repeats the module import"
    );
}

/// No warning where nothing repeats: an alias, distinct members, two
/// modules' imports.
#[test]
fn e255_an_alias_or_a_distinct_member_is_no_duplicate() {
    let body = "\nfun main() {\n\tlet _: Option<Json> = None;\n\tlet _: Option<Document> = None;\n\tlet _: Option<JsonValue> = None;\n}\n";
    assert!(warnings(&format!(
        "import std::json::Json;\nimport std::json::Json as Document;\nimport std::json::JsonValue;\n{body}"
    ))
    .is_empty());
}

/// The fix: the organize action's edit for the run — applied, the warning is
/// gone and the run is E251's merge.
#[test]
fn e255_the_quick_fix_is_the_organize_edit_and_clears_the_warning() {
    let source =
        format!("import std::json::{{ self, Json, JsonValue }};\nimport std::json::Json;\n{BODY}");
    let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    let (span, _) = document.duplicate_import_warnings()[0].clone();
    let fixes = document.quickfixes(program, span);
    let fix = fixes
        .iter()
        .find(|fix| fix.title == "Remove the duplicate import (Organize Imports)")
        .expect("the fix is offered");
    let mut fixed = source.clone();
    fixed.replace_range(fix.span.into_range(), &fix.replacement);
    assert_eq!(
        fixed,
        format!("import std::json::{{ self, Json, JsonValue }};\n{BODY}")
    );
    assert!(warnings(&fixed).is_empty());
}
