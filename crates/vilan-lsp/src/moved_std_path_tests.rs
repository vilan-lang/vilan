//! A154's editor half: a path through a std module that moved under a
//! namespace (`std::dom`, now `std::web::dom`) publishes with its stable code
//! (`std-path/moved`) and carries a quick fix that writes the new path at the
//! old segment. The refusal and the fix's data are
//! `vilan_core::parsing::{moved_std_module_message, moved_std_module_fix}`,
//! read off the one table (`MOVED_STD_MODULES`); this is the code action over
//! it.

use std::path::Path;

use tower_lsp::lsp_types::{NumberOrString, Url};

use crate::document::Document;
use crate::document::tests::std_root;
use crate::publish::PublishState;

/// `source` with the one quick fix titled `title` applied.
fn fixed(source: &str, title: &str) -> String {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    let mut fixes: Vec<_> = document
        .diagnostics
        .iter()
        .flat_map(|diagnostic| document.quickfixes(program, diagnostic.span))
        .filter(|fix| fix.title == title)
        .map(|fix| (fix.span, fix.replacement))
        .collect();
    fixes.dedup();
    assert_eq!(
        fixes.len(),
        1,
        "exactly one `{title}` fix is offered for {source:?}"
    );
    let (span, replacement) = fixes.remove(0);
    let mut text = source.to_string();
    text.replace_range(span.into_range(), &replacement);
    text
}

fn clean(source: &str) {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    assert!(
        document.diagnostics.is_empty(),
        "the fixed program checks clean: {source:?} {:?}",
        document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_moved_module_segment_is_rewritten_in_place() {
    let after = fixed(
        "import std::dom::create_element;\n\nfun main() {\n\tlet _ = create_element(\"div\");\n}\n",
        "Write `std::web::dom`",
    );
    assert_eq!(
        after,
        "import std::web::dom::create_element;\n\nfun main() {\n\tlet _ = create_element(\"div\");\n}\n"
    );
    clean(&after);
}

#[test]
fn a_two_segment_destination_keeps_the_rest_of_the_path() {
    // `rpc_server` becomes TWO segments, and the brace list after it stays.
    let after = fixed(
        "import std::reactive::SignalCell;\nimport std::delta::{ ListCell };\n\nfun main() {}\n",
        "Write `std::reactive::delta`",
    );
    assert_eq!(
        after,
        "import std::reactive::SignalCell;\nimport std::reactive::delta::{ ListCell };\n\nfun main() {}\n"
    );
    clean(&after);
}

#[test]
fn the_old_web_prelude_path_is_rewritten_at_web() {
    // `std::web` is the namespace now, so `web` resolves and `Signal` misses;
    // the refusal still anchors at `web`, which is what the fix replaces.
    let after = fixed(
        "import std::web::Signal;\n\nfun main() {\n\tlet _ = Signal::new(1);\n}\n",
        "Write `std::web::prelude`",
    );
    assert_eq!(
        after,
        "import std::web::prelude::Signal;\n\nfun main() {\n\tlet _ = Signal::new(1);\n}\n"
    );
    clean(&after);
}

/// E268: a brace list under the old web-prelude path that also names one of
/// `std::web`'s children — rewriting `web` would carry `dom::..` under the
/// prelude and break it, so the edit writes `prelude::` before the prelude
/// name instead, and the child stays where it resolves.
#[test]
fn a_mixed_web_list_moves_only_the_prelude_names() {
    let after = fixed(
        "import std::web::{ Signal, dom::create_element };\n\nfun main() {\n\tlet _ = Signal::new(1);\n\tlet _ = create_element(\"div\");\n}\n",
        "Write `prelude::` before `Signal` (the web prelude is `std::web::prelude`)",
    );
    assert_eq!(
        after,
        "import std::web::{ prelude::Signal, dom::create_element };\n\nfun main() {\n\tlet _ = Signal::new(1);\n\tlet _ = create_element(\"div\");\n}\n"
    );
    clean(&after);
}

/// E269: the old prelude imported AS A MODULE — a bare `import std::web;`
/// and a `self` in a brace list under the old path — is the moved-path
/// refusal, with the fix that keeps the binding's name.
#[test]
fn the_old_web_prelude_as_a_module_keeps_its_name() {
    let after = fixed(
        "import std::web;\n\nfun main() {\n\tlet _ = web::Signal::new(1);\n}\n",
        "Write `std::web::prelude as web`",
    );
    assert_eq!(
        after,
        "import std::web::prelude as web;\n\nfun main() {\n\tlet _ = web::Signal::new(1);\n}\n"
    );
    clean(&after);
    let after = fixed(
        "import std::web::{ self, Signal };\n\nfun main() {\n\tlet _ = web::Signal::new(1);\n\tlet _ = Signal::new(2);\n}\n",
        "Write `prelude as web` for `self` and `prelude::` before the prelude's names (the web prelude is `std::web::prelude`)",
    );
    assert_eq!(
        after,
        "import std::web::{ prelude as web, prelude::Signal };\n\nfun main() {\n\tlet _ = web::Signal::new(1);\n\tlet _ = Signal::new(2);\n}\n"
    );
    clean(&after);
}

/// The diagnostic is published with its stable code, which a client (and the
/// fix) can key on whatever the message's wording becomes.
#[test]
fn the_diagnostic_is_published_with_its_code() {
    let path = std::env::temp_dir().join(format!(
        "vilan-a154-code-{}-{:?}.vl",
        std::process::id(),
        std::thread::current().id()
    ));
    let document = Document::analyze(
        "import std::store::Store;\n\nfun main() {}\n",
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
        vec![Some(NumberOrString::String("std-path/moved".to_string()))]
    );
}
