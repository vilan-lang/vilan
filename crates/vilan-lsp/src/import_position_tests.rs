//! E250: the import quick fix at EVERY position an unresolved name can stand
//! in — the census, one pin per position.
//!
//! The fix keyed on two message shapes (`cannot find '…'`, `cannot find type
//! '…'`), so three positions had none: a struct literal's head (`unknown
//! struct: Header`), a pattern's path head (`cannot find 'JsonKind::Null'` —
//! the PATH, which no module declares; the head is what an import brings), and
//! an `impl`'s `with` trait (`cannot find trait 'Hashable'`).
//!
//! Two positions of the item's list are not positions at all, and are pinned
//! as what they are: `[derive(X)]` resolves a std derive by name with no
//! import (nothing to fix), and an element tag is a string lowered to
//! `view("tag")` (`<div>` names no declaration). A `let` takes a binding or a
//! tuple pattern only, so it has no path head to be unresolved.

use std::path::Path;

use crate::document::Document;
use crate::document::tests::std_root;

/// The import fixes offered over the first diagnostic that mentions `name`.
fn import_fixes(source: &str, name: &str) -> Vec<String> {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    let diagnostic = document
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.msg.contains(name))
        .unwrap_or_else(|| {
            panic!(
                "a diagnostic naming `{name}`: {:?}",
                document
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.msg.clone())
                    .collect::<Vec<_>>()
            )
        });
    document
        .quickfixes(program, diagnostic.span)
        .into_iter()
        .map(|fix| fix.title)
        .filter(|title| title.starts_with("Import "))
        .collect()
}

fn assert_import_offered(position: &str, source: &str, name: &str, module: &str) {
    let fixes = import_fixes(source, name);
    let wanted = format!("Import `{name}` from {module}");
    assert!(
        fixes.contains(&wanted),
        "{position}: `{wanted}` must be offered — got {fixes:?}"
    );
}

#[test]
fn an_expression() {
    assert_import_offered(
        "an expression",
        "fun main() {\n\tlet h = Header;\n}\n",
        "Header",
        "std::fetch",
    );
}

#[test]
fn a_type_annotation() {
    assert_import_offered(
        "an annotation",
        "fun main() {\n\tlet values: List<i32> = [];\n\tlet h: Header = panic(\"x\");\n}\n",
        "Header",
        "std::fetch",
    );
}

/// The owner's first case (kolt `client.vl:60`).
#[test]
fn a_struct_literal_head() {
    assert_import_offered(
        "a struct literal head",
        "fun main() {\n\tlet h = Header { name = \"a\", value = \"b\" };\n}\n",
        "Header",
        "std::fetch",
    );
}

/// The owner's second case: a `match` arm's path head.
#[test]
fn a_match_patterns_head() {
    assert_import_offered(
        "a match pattern",
        "fun pick(kind: str) {\n\tmatch kind {\n\t\tJsonKind::Null => {}\n\t\t_ => {}\n\t}\n}\n",
        "JsonKind",
        "std::json",
    );
}

#[test]
fn an_is_patterns_head() {
    assert_import_offered(
        "an `is` pattern",
        "fun pick(kind: str) {\n\tif kind is JsonKind::Null {}\n}\n",
        "JsonKind",
        "std::json",
    );
}

#[test]
fn an_impl_subject() {
    assert_import_offered(
        "an impl subject",
        "impl Header {\n\tfun label(self): str {\n\t\tself.name\n\t}\n}\n",
        "Header",
        "std::fetch",
    );
}

#[test]
fn an_impl_with_trait() {
    assert_import_offered(
        "an impl's `with` trait",
        "struct Point {\n\tx: i32,\n}\n\nimpl Point with Hashable {\n\tfun hash(self): i32 {\n\t\tself.x\n\t}\n}\n",
        "Hashable",
        "std::hash",
    );
}

#[test]
fn a_generic_argument() {
    assert_import_offered(
        "a generic argument",
        "fun main() {\n\tlet values: List<Header> = [];\n}\n",
        "Header",
        "std::fetch",
    );
}

#[test]
fn a_bound() {
    assert_import_offered(
        "a bound",
        "fun keyed<T: Hashable>(value: T): T {\n\tvalue\n}\n",
        "Hashable",
        "std::hash",
    );
}

#[test]
fn a_static_calls_receiver() {
    assert_import_offered(
        "a static call's receiver",
        "fun main() {\n\tlet map = HashMap::new();\n}\n",
        "HashMap",
        "std::hash_map",
    );
}

#[test]
fn a_variants_type() {
    assert_import_offered(
        "a variant's type",
        "fun main() {\n\tlet kind = JsonKind::Null;\n}\n",
        "JsonKind",
        "std::json",
    );
}

#[test]
fn a_parameter_and_a_return_type() {
    assert_import_offered(
        "a parameter type",
        "fun pick(map: HashMap<str, i32>) {}\n",
        "HashMap",
        "std::hash_map",
    );
    assert_import_offered(
        "a return type",
        "fun make(): Header {\n\tpanic(\"x\")\n}\n",
        "Header",
        "std::fetch",
    );
}

/// Not a position: a std derive resolves by name, with no import to add.
#[test]
fn a_derive_needs_no_import() {
    let document = Document::analyze(
        "[derive(Hashable)]\nstruct Point {\n\tx: i32,\n}\n\nfun main() {}\n",
        &std_root(),
        Path::new("test.vl"),
    );
    assert!(
        document.diagnostics.is_empty(),
        "{:?}",
        document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
}
