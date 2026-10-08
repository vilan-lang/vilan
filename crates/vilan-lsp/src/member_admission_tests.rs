//! E249: the members a BLANKET impl gives a receiver are offered by member
//! completion — the impls the solver's own selection admits for the receiver's
//! type (`impl_select::applying_implementations`, what emission reads), not a
//! second applicability rule kept in the editor.
//!
//! The find: `switch_some` and `and_then` (`impl type F: Flow<Option<type T>>`)
//! were missing on a `MemoCell<Option<i32>>` that checks clean against them.
//! The owner suspected the nested `type T` binder; the editor's member table
//! grouped impls by the NOMINAL their subject names, so a blanket — whose
//! subject is a binder, not a nominal — was never offered at all, nested or
//! not: `distinct` (`impl type F: Flow<type T: PartialEq>`) and `then_some`
//! (`impl type F: Flow<bool>`) were missing alike, on every receiver.

use std::path::Path;

use crate::document::Document;
use crate::document::tests::std_root;

const IMPORTS: &str = "import std::reactive::{ MemoCell, SignalCell };\n\n";

/// The labels offered at the `~` cursor in `body`, inside a function taking
/// the parameters `parameters`.
fn offered(parameters: &str, body: &str) -> Vec<String> {
    // The premise, on the same receiver with nothing typed after it: the
    // fixture checks clean.
    let premise = format!("{IMPORTS}fun pick({parameters}) {{\n\tlet _ = m;\n}}\n");
    let checked = Document::analyze(&premise, &std_root(), Path::new("test.vl"));
    assert!(
        checked.diagnostics.is_empty(),
        "the fixture must check clean: {:?}",
        checked
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>(),
    );
    let source = format!("{IMPORTS}fun pick({parameters}) {{\n{body}}}\n");
    let offset = source.find('~').expect("the pin source needs a `~` cursor");
    let text = source.replace('~', "");
    let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
    document
        .completion(offset)
        .into_iter()
        .map(|completion| completion.label)
        .collect()
}

fn assert_offers(parameters: &str, members: &[&str], absent: &[&str]) {
    let found = offered(parameters, "\tm.~;\n");
    for member in members {
        assert!(
            found.iter().any(|label| label == member),
            "`m: {parameters}` must offer `{member}`: {found:?}",
        );
    }
    for member in absent {
        assert!(
            !found.iter().any(|label| label == member),
            "`m: {parameters}` must NOT offer `{member}` — its blanket does not admit it: {found:?}",
        );
    }
}

/// The owner's receiver.
#[test]
fn a_memo_cell_of_an_option_offers_the_option_flow_blankets() {
    assert_offers(
        "m: MemoCell<Option<i32>>",
        &["switch_some", "and_then", "flatten", "distinct", "switch"],
        &["then_some"],
    );
}

/// The same blankets on another `Flow`: the admission is the flow's, not
/// `MemoCell`'s.
#[test]
fn a_signal_cell_of_an_option_offers_them_too() {
    assert_offers(
        "m: SignalCell<Option<i32>>",
        &["switch_some", "and_then", "distinct"],
        &["then_some"],
    );
}

/// `impl type F: Flow<bool>` — the blanket over a CONCRETE argument.
#[test]
fn a_memo_cell_of_a_bool_offers_then_some() {
    assert_offers(
        "m: MemoCell<bool>",
        &["then_some", "distinct"],
        &["switch_some", "and_then"],
    );
}

/// The negative: a `Flow<i32>` is not a `Flow<Option<T>>`.
#[test]
fn a_memo_cell_of_a_scalar_offers_none_of_the_option_blankets() {
    assert_offers(
        "m: MemoCell<i32>",
        &["distinct", "switch", "track"],
        &["switch_some", "and_then", "flatten", "then_some"],
    );
}

/// A user blanket over a bound, in the file itself — `impl type T: PartialEq
/// with Same` (B511's shape): offered on a type that is `PartialEq`, and the
/// receiver need not be a flow at all.
#[test]
fn a_user_blanket_over_a_bound_is_offered_where_the_bound_holds() {
    let source = "import std::compare::PartialEq;\n\ntrait Same {\n\tfun same(self, other: Self): bool;\n}\n\n\
         impl type T: PartialEq with Same {\n\tfun same(self, other: Self): bool {\n\t\tself == other\n\t}\n}\n\n\
         fun main() {\n\tlet count = 3;\n\tlet _ = count.~;\n}\n";
    let offset = source.find('~').expect("a cursor");
    let text = source.replace('~', "");
    let premise = text.replace("count.;", "count.same(4);");
    let checked = Document::analyze(&premise, &std_root(), Path::new("test.vl"));
    assert!(
        checked.diagnostics.is_empty(),
        "the premise checks clean: {:?}",
        checked
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>(),
    );
    let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
    let found: Vec<String> = document
        .completion(offset)
        .into_iter()
        .map(|completion| completion.label)
        .collect();
    assert!(found.iter().any(|label| label == "same"), "{found:?}");
}

/// Hover on a blanket member's call already answered — the analyzer resolves
/// the call and hover reads its resolution — and stays answered: the gap was
/// completion's alone. (There is no signature help to miss: the server
/// advertises none.)
#[test]
fn hover_on_a_blanket_members_call_names_it() {
    let source = format!("{IMPORTS}fun pick(m: MemoCell<i32>) {{\n\tlet _ = m.distinct();\n}}\n");
    let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
    let at = source.find("distinct").expect("the call") + 3;
    let hover = document.hover(at).expect("a hover on the member");
    assert!(
        hover.contains("impl type F: Flow<type T: PartialEq>") && hover.contains("fun distinct"),
        "{hover}"
    );
}

/// E253, re-read: a call taking a closure with a `context` clause —
/// `switch`, `switch_some`, `and_then`, blanket or not — hovers and navigates
/// like any other. The find's own program was refused (`MemoCell::new` does
/// not exist), and THAT is what left the call unrecorded: see below.
#[test]
fn hover_on_a_call_taking_a_context_closure_names_the_member() {
    for (receiver, call, member) in [
        (
            "MemoCell<Option<i32>>",
            "m.switch_some(|value| SignalCell::new(value))",
            "fun switch_some",
        ),
        (
            "MemoCell<Option<i32>>",
            "m.and_then(|value| SignalCell::new(Some(value)))",
            "fun and_then",
        ),
        (
            "MemoCell<i32>",
            "m.switch(|value| SignalCell::new(value))",
            "fun switch",
        ),
    ] {
        let source = format!("{IMPORTS}fun pick(m: {receiver}) {{\n\tlet _ = {call};\n}}\n");
        let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
        assert!(
            document.diagnostics.is_empty(),
            "the premise: {call} checks clean {:?}",
            document
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.msg.clone())
                .collect::<Vec<_>>()
        );
        let name = member.trim_start_matches("fun ");
        let at = source.find(&format!(".{name}(")).expect("the call") + 3;
        let hover = document.hover(at).expect("a hover on the member");
        assert!(hover.contains(member), "{call}: {hover}");
        assert!(document.definition(at).is_some(), "{call}: a definition");
    }
}

/// E253's real cause: a method call whose CLOSURE argument's body is refused
/// never wires — the closure's return is the method's generic, so the call
/// defers on it to the end — and left no record behind but its span. Hover
/// and go-to-definition on the method answered nothing exactly while the
/// author was fixing the closure. A refused argument that is not a closure
/// never had the gap (the call wires, and the argument carries the error).
#[test]
fn hover_on_a_call_whose_closure_is_refused_names_the_member() {
    for (receiver, call, member) in [
        (
            "MemoCell<Option<i32>>",
            "m.switch_some(|value| MemoCell::new(value))",
            "fun switch_some",
        ),
        ("Option<i32>", "m.map(|value| nope(value))", "fun map"),
        (
            "List<i32>",
            "m.map(|value| { let doubled = value * 2; nope(doubled) })",
            "fun map",
        ),
    ] {
        let source = format!("{IMPORTS}fun pick(m: {receiver}) {{\n\tlet _ = {call};\n}}\n");
        let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
        assert!(
            !document.diagnostics.is_empty(),
            "the premise: {call}'s closure is refused"
        );
        let name = member.trim_start_matches("fun ");
        let at = source.find(&format!(".{name}(")).expect("the call") + 3;
        let hover = document
            .hover(at)
            .unwrap_or_else(|| panic!("{call}: a hover on the member"));
        assert!(hover.contains(member), "{call}: {hover}");
        assert!(document.definition(at).is_some(), "{call}: a definition");
    }
}
