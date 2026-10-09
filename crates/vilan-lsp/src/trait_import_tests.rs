//! B515's editor half (B535): a trait method called in a file that does not
//! import its trait carries a quick fix that writes the import — on B535's
//! refusal (B515's warning until v0.45.0, R-c: the call would resolve because
//! another loaded module imports the trait) and on the no-method steer
//! (nothing loaded it) alike — one trait at a time, and every trait the file
//! needs at once. The refusal publishes with its stable code. The message's
//! import is read by `vilan_ide::trait_import` (through the analyzer's own
//! `trait_scope_import`); the candidate scan the add-import fix uses decides
//! the path written.

use std::path::Path;

use tower_lsp::lsp_types::{NumberOrString, Url};

use crate::document::tests::std_root;
use crate::document::{Document, QuickFix};
use crate::publish::PublishState;

/// Every quick fix offered over each of `document`'s diagnostics and
/// warnings, asked at the finding's own span.
fn offered(document: &Document) -> Vec<QuickFix> {
    let program = document.program.as_ref().expect("a program");
    document
        .diagnostics
        .iter()
        .chain(&document.warnings)
        .flat_map(|finding| document.quickfixes(program, finding.span))
        .collect()
}

fn titles(document: &Document) -> Vec<String> {
    offered(document).into_iter().map(|fix| fix.title).collect()
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
        titles(&document)
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

/// The premise of every pin below: `std::markdown` imports `Display` and
/// `Hashable`, so their methods resolve in a file that imports neither.
const LOADS_THE_TRAITS: &str = "import std::markdown::parse;\n\n";

#[test]
fn b535s_refusal_imports_the_trait() {
    let source = format!(
        "{LOADS_THE_TRAITS}fun main() {{\n\tlet _ = parse(\"x\");\n\tprint(42.to_string());\n}}\n"
    );
    let after = fixed(&source, "Import `Display` from std::display");
    assert_eq!(
        after,
        "import std::display::Display;\nimport std::markdown::parse;\n\nfun main() {\n\tlet _ = parse(\"x\");\n\tprint(42.to_string());\n}\n"
    );
    clean(&after);
}

/// Nothing loaded the trait's module: the call does not resolve, and the
/// steer's import is the same fix.
#[test]
fn the_no_method_steer_imports_the_trait() {
    let after = fixed(
        "fun main() {\n\tprint(42.to_string());\n}\n",
        "Import `Display` from std::display",
    );
    assert_eq!(
        after,
        "import std::display::Display;\nfun main() {\n\tprint(42.to_string());\n}\n"
    );
    clean(&after);
}

/// The file-wide fix: every trait the file calls without importing, in one
/// edit, offered from each of the sites — two calls of one trait import it
/// once.
#[test]
fn every_trait_the_file_needs_is_imported_at_once() {
    let source = format!(
        "{LOADS_THE_TRAITS}fun main() {{\n\tlet _ = parse(\"x\");\n\tprint(42.to_string());\n\tprint(7.to_string());\n\tlet _ = \"a\".hash();\n}}\n"
    );
    let title = "Import all 2 traits this file calls";
    let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    let refusals: Vec<_> = document
        .diagnostics
        .iter()
        .filter(|diagnostic| vilan_core::analyzer::trait_scope_import(&diagnostic.msg).is_some())
        .collect();
    assert_eq!(refusals.len(), 3, "the premise: three calls are refused");
    for refusal in refusals {
        let offered: Vec<String> = document
            .quickfixes(program, refusal.span)
            .into_iter()
            .map(|fix| fix.title)
            .collect();
        assert!(
            offered.iter().any(|offered| offered == title),
            "{offered:?}"
        );
    }
    let after = fixed(&source, title);
    assert!(
        after.starts_with(
            "import std::display::Display;\nimport std::hash::Hashable;\nimport std::markdown::parse;\n"
        ),
        "{after}"
    );
    clean(&after);
}

/// One trait, however many calls: the file-wide fix would be the per-site
/// fix under a longer name, and is not offered.
#[test]
fn one_trait_offers_no_file_wide_fix() {
    let source = format!(
        "{LOADS_THE_TRAITS}fun main() {{\n\tlet _ = parse(\"x\");\n\tprint(42.to_string());\n\tprint(7.to_string());\n}}\n"
    );
    let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
    assert!(
        titles(&document)
            .iter()
            .all(|title| !title.starts_with("Import all ")),
        "{:?}",
        titles(&document)
    );
}

/// The source action "Add All Missing Imports" writes the traits too, beside
/// the names a file cannot find.
#[test]
fn add_all_missing_imports_writes_the_traits() {
    let source = format!(
        "{LOADS_THE_TRAITS}fun main() {{\n\tlet _ = parse(\"x\");\n\tprint(42.to_string());\n\tlet _ = \"a\".hash();\n}}\n"
    );
    let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
    let program = document.program.as_ref().expect("a program");
    let (span, replacement) = document
        .add_all_missing_imports_edit(program)
        .expect("an edit");
    let mut after = source.clone();
    after.replace_range(span.into_range(), &replacement);
    clean(&after);
}

/// B515's warning publishes with its stable code; the no-method steer, an
/// error of another kind, carries none.
#[test]
fn the_refusal_is_published_with_its_code() {
    let path = std::env::temp_dir().join(format!(
        "vilan-b515-code-{}-{:?}.vl",
        std::process::id(),
        std::thread::current().id()
    ));
    let document = Document::analyze(
        &format!(
            "{LOADS_THE_TRAITS}fun main() {{\n\tlet _ = parse(\"x\");\n\tprint(42.to_string());\n}}\n"
        ),
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
            "trait-scope/not-imported".to_string()
        ))]
    );
}

/// A scratch package on disk: `vilan.toml` and `src/<name>` for each file.
struct Package {
    directory: std::path::PathBuf,
}

impl Package {
    fn new(tag: &str, files: &[(&str, &str)]) -> Package {
        let directory = std::env::temp_dir().join(format!(
            "vilan-b515-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("src")).expect("a scratch package");
        std::fs::write(directory.join("vilan.toml"), "[package]\nname = \"app\"\n")
            .expect("a manifest");
        for (relative, contents) in files {
            let path = directory.join("src").join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(path, contents).expect("a source file");
        }
        Package { directory }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.directory.join("src").join(name)
    }
}

impl Drop for Package {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// A trait in a NESTED std module (`std::reactive::delta`'s `SequenceCell`),
/// reached through a sibling that imports its module: the warning's own
/// statement named `pkg::delta::SequenceCell`, which resolves nowhere (filed
/// from editor-47). The fix writes the path the candidate scan finds — since
/// B572 the shortest PUBLIC one, `std::reactive`, which re-exports it (the
/// book's spelling), where it wrote the declaring `std::reactive::delta`.
#[test]
fn a_nested_std_trait_is_imported_from_its_public_module() {
    let package = Package::new(
        "nested-std",
        &[(
            "helper.vl",
            "import std::reactive::delta::ListCell;\n\nexport fun make(): ListCell<i32> {\n\tListCell::new()\n}\n",
        )],
    );
    let source =
        "import pkg::helper::make;\n\nfun main() {\n\tlet cell = make();\n\tcell.push(1);\n}\n";
    let path = package.path("main.vl");
    let document = Document::analyze(source, &std_root(), &path);
    assert!(
        document
            .diagnostics
            .iter()
            .any(|refusal| refusal.msg.contains("does not import `SequenceCell`")),
        "the premise: the call is refused {:?}",
        document
            .diagnostics
            .iter()
            .map(|refusal| refusal.msg.clone())
            .collect::<Vec<_>>()
    );
    let fixes: Vec<QuickFix> = offered(&document)
        .into_iter()
        .filter(|fix| fix.title.starts_with("Import `SequenceCell` from "))
        .collect();
    assert_eq!(
        fixes
            .iter()
            .map(|fix| fix.title.as_str())
            .collect::<Vec<_>>(),
        ["Import `SequenceCell` from std::reactive"]
    );
    let mut after = source.to_string();
    after.replace_range(fixes[0].span.into_range(), &fixes[0].replacement);
    let fixed = Document::analyze(&after, &std_root(), &path);
    assert!(
        fixed.diagnostics.is_empty() && fixed.warnings.is_empty(),
        "{after}: {:?}",
        fixed
            .diagnostics
            .iter()
            .chain(&fixed.warnings)
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
}

/// The same for a trait in a NESTED module of the package (E267's walk): the
/// warning's statement names `pkg::shapes::Area`, the fix writes the path
/// that resolves.
#[test]
fn a_nested_package_trait_is_imported_from_where_it_is_declared() {
    let package = Package::new(
        "nested-pkg",
        &[
            (
                "geo/shapes.vl",
                "export trait Area {\n\tfun area(self): i32;\n}\n\nexport impl i32 with Area {\n\tfun area(self): i32 {\n\t\tself * self\n\t}\n}\n",
            ),
            (
                "helper.vl",
                "import pkg::geo::shapes::Area;\n\nexport fun twice(x: i32): i32 {\n\tx.area() * 2\n}\n",
            ),
        ],
    );
    let source = "import pkg::helper::twice;\n\nfun main() {\n\tprint(twice(2) + 3.area());\n}\n";
    let path = package.path("main.vl");
    let document = Document::analyze(source, &std_root(), &path);
    let fixes: Vec<QuickFix> = offered(&document)
        .into_iter()
        .filter(|fix| fix.title.starts_with("Import `Area` from "))
        .collect();
    assert_eq!(
        fixes
            .iter()
            .map(|fix| fix.title.as_str())
            .collect::<Vec<_>>(),
        ["Import `Area` from pkg::geo::shapes"]
    );
    let mut after = source.to_string();
    after.replace_range(fixes[0].span.into_range(), &fixes[0].replacement);
    let fixed = Document::analyze(&after, &std_root(), &path);
    assert!(
        fixed.diagnostics.is_empty() && fixed.warnings.is_empty(),
        "{after}: {:?}",
        fixed
            .diagnostics
            .iter()
            .chain(&fixed.warnings)
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>()
    );
}

/// B572's editor half: a std name a facade re-exports is imported from the
/// facade — the add-import fix for an unresolved `Store` writes
/// `std::reactive::store`, the module the steer names and the book imports
/// from, not `store_core`, the internal module that declares it.
#[test]
fn a_reexported_std_name_is_imported_from_its_public_module() {
    let package = Package::new("facade", &[]);
    let source = "fun main() {\n\tlet store = Store::new(1);\n\tlet _ = store;\n}\n";
    let path = package.path("main.vl");
    let document = Document::analyze(source, &std_root(), &path);
    let titles: Vec<String> = offered(&document)
        .into_iter()
        .map(|fix| fix.title)
        .filter(|title| title.starts_with("Import `Store` from "))
        .collect();
    assert_eq!(titles, ["Import `Store` from std::reactive::store"]);
}
