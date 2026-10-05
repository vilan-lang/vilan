//! B520's editor half: a foreign spelling — `return`, `fn`, `function`,
//! `func`, `def`, a `->` return type, a `->` in a closure type — publishes
//! with its stable code (`foreign-spelling/<what was written>`) and carries a
//! quick fix that writes the vilan spelling. syntax-46 built the diagnostic
//! and the fix's data (`vilan_core::parsing::{ForeignSpelling,
//! foreign_spelling_fix}`); this is the code action over it.

use std::path::Path;

use tower_lsp::lsp_types::{NumberOrString, Url};

use crate::document::Document;
use crate::document::tests::std_root;
use crate::publish::PublishState;

/// `source` with every quick fix titled `title` applied, one at a time.
fn fixed(source: &str, title: &str) -> String {
    let document = Document::analyze(source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    let mut text = source.to_string();
    let mut fixes: Vec<_> = document
        .diagnostics
        .iter()
        .flat_map(|diagnostic| document.quickfixes(program, diagnostic.span))
        .filter(|fix| fix.title == title)
        .map(|fix| (fix.span, fix.replacement))
        .collect();
    assert!(
        !fixes.is_empty(),
        "a `{title}` fix is offered for {source:?}"
    );
    fixes.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
    fixes.dedup();
    for (span, replacement) in fixes {
        text.replace_range(span.into_range(), &replacement);
    }
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
fn return_becomes_ret() {
    let after = fixed(
        "fun one(): i32 {\n\treturn 1;\n}\n\nfun main() {}\n",
        "Write `ret`",
    );
    assert_eq!(after, "fun one(): i32 {\n\tret 1;\n}\n\nfun main() {}\n");
    clean(&after);
}

#[test]
fn each_foreign_function_word_becomes_fun() {
    for word in ["fn", "function", "func", "def"] {
        let after = fixed(&format!("{word} main() {{}}\n"), "Write `fun`");
        assert_eq!(after, "fun main() {}\n", "{word}");
        clean(&after);
    }
}

#[test]
fn a_return_type_arrow_becomes_a_colon() {
    let after = fixed(
        "fun one() -> i32 {\n\t1\n}\n\nfun main() {}\n",
        "Write `:` for the return type",
    );
    assert_eq!(after, "fun one(): i32 {\n\t1\n}\n\nfun main() {}\n");
    clean(&after);
}

#[test]
fn a_closure_types_arrow_is_removed() {
    let after = fixed(
        "fun apply(f: |i32| -> i32): i32 {\n\tf(1)\n}\n\nfun main() {}\n",
        "Remove the `->`",
    );
    assert_eq!(
        after,
        "fun apply(f: |i32| i32): i32 {\n\tf(1)\n}\n\nfun main() {}\n"
    );
    clean(&after);
}

/// The diagnostic is published with its stable code, which a client (and the
/// fix) can key on whatever the message's wording becomes.
#[test]
fn the_diagnostic_is_published_with_its_code() {
    let path = std::env::temp_dir().join(format!("vilan-b520-code-{}.vl", std::process::id()));
    let document = Document::analyze("fn main() {}\n", &std_root(), &path);
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
            "foreign-spelling/fn".to_string()
        ))]
    );
}

/// M104 meets B520: a module served from its entry's world carries the same
/// fix — the module's parse diagnostic reaches the world with the parser's own
/// message, and the view reads the diagnostics attributed to it.
#[test]
fn a_module_served_from_its_entrys_world_offers_the_fix() {
    let directory = std::env::temp_dir().join(format!(
        "vilan-b520-world-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("src")).expect("a package");
    let model = "export fn helper(): i32 {\n\t1\n}\n";
    for (relative, text) in [
        (
            "vilan.toml",
            "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
        ),
        (
            "src/client.vl",
            "import pkg::model::helper;\n\nfun main() {\n\tlet _ = helper();\n}\n",
        ),
        ("src/server.vl", "fun main() {}\n"),
        ("src/model.vl", model),
    ] {
        std::fs::write(directory.join(relative), text).expect("a source");
    }
    let client = directory.join("src/client.vl");
    let client_text = std::fs::read_to_string(&client).expect("the entry");
    let world = Document::analyze(&client_text, &std_root(), &client);
    let view = Document::view_of(
        &world,
        &client,
        &directory.join("src/model.vl"),
        model,
        Vec::new(),
    )
    .expect("the world serves the module");
    let program = view.program.as_ref().expect("a program");
    let fixes: Vec<(String, String)> = view
        .diagnostics
        .iter()
        .flat_map(|diagnostic| view.quickfixes(program, diagnostic.span))
        .map(|fix| (fix.title, model[fix.span.into_range()].to_string()))
        .collect();
    assert!(
        fixes.contains(&("Write `fun`".to_string(), "fn".to_string())),
        "{fixes:?}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}
