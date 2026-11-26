//! B711 S1a (`struct-keys.md`): a file using struct keys in the editor. Keys
//! get no completion or hover of their own yet (S1c); these pins hold that
//! nothing crashes or answers nonsense on `Task.` / `Task.title`, and that an
//! edit to a key literal is diagnosed and cleared through the editing path —
//! the live buffer's ranged edit, then the analysis of what it holds.

use std::path::Path;

use super::Document;
use super::tests::{on_big_stack, std_root};

const SOURCE: &str = "struct Meta {\n\towner: str,\n}\n\nstruct Task {\n\ttitle: str,\n\tmeta: Meta,\n}\n\n\
     fun main() {\n\tmut t = Task { title = \"a\", meta = Meta { owner = \"b\" } };\n\
     \tlet k = Task.title;\n\tt[Task.meta.owner] = \"c\";\n\tprint(t[k]);\n}\n";

fn analyzed(text: &str) -> Document {
    Document::analyze(text, &std_root(), Path::new("test.vl"))
}

fn messages(document: &Document) -> Vec<String> {
    document
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.msg.clone())
        .collect()
}

/// Replace `start..end` of the live buffer with `text` as the editor does (a
/// ranged change), then analyze what the buffer now holds.
fn edited(document: &mut Document, start: usize, end: usize, text: &str) -> Document {
    let range = tower_lsp::lsp_types::Range::new(
        document.line_index.position(start),
        document.line_index.position(end),
    );
    document.apply_change(Some(range), text);
    analyzed(&document.text.clone())
}

#[test]
fn keys_a_file_using_keys_analyzes_clean() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
    });
}

/// A field renamed inside a key literal is refused with the struct's name and
/// the field it was likely meant to be; restoring it clears the refusal.
#[test]
fn keys_editing_a_key_literals_field_is_diagnosed_then_cleared() {
    on_big_stack(|| {
        let mut document = analyzed(SOURCE);
        assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
        let at = SOURCE.find("Task.title;").expect("the literal") + "Task.".len();
        let broken = edited(&mut document, at, at + "title".len(), "titl");
        assert_eq!(
            messages(&broken),
            vec!["struct 'Task' has no field 'titl'; did you mean `title`?".to_string()],
        );
        let mut document = broken;
        let at = document
            .text
            .find("Task.titl;")
            .expect("the edited literal")
            + "Task.".len();
        let fixed = edited(&mut document, at, at + "titl".len(), "title");
        assert!(fixed.diagnostics.is_empty(), "{:?}", messages(&fixed));
        assert_eq!(fixed.text, SOURCE);
    });
}

/// The second step of a path, edited the same way.
#[test]
fn keys_editing_a_paths_second_step_is_diagnosed_on_its_struct() {
    on_big_stack(|| {
        let mut document = analyzed(SOURCE);
        let at = SOURCE.find("Task.meta.owner").expect("the path") + "Task.meta.".len();
        let broken = edited(&mut document, at, at + "owner".len(), "ownr");
        assert_eq!(
            messages(&broken),
            vec!["struct 'Meta' has no field 'ownr'; did you mean `owner`?".to_string()],
        );
    });
}

/// Hover over a key literal's root and field, and completion after `Task.`,
/// answer without a panic, and what they answer is about `Task`.
#[test]
fn keys_hover_and_completion_on_a_key_literal_do_not_crash() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let literal = SOURCE.find("Task.title;").expect("the literal");
        let root = document.hover(literal + 1);
        if let Some(root) = &root {
            assert!(root.contains("Task"), "{root}");
        }
        let field = document.hover(literal + "Task.".len() + 1);
        if let Some(field) = &field {
            assert!(
                field.contains("Key<Task, str>") || field.contains("title"),
                "{field}"
            );
        }
        let completed = document.completion(literal + "Task.".len());
        for candidate in &completed {
            assert!(!candidate.label.is_empty());
        }
        // Mid-typing: `Task.` with nothing after it.
        let typing = SOURCE.replace("let k = Task.title;", "let k = Task.;");
        let typing_document = analyzed(&typing);
        let dot = typing.find("Task.;").expect("the dot") + "Task.".len();
        let _ = typing_document.completion(dot);
        let _ = typing_document.hover(dot - 1);
    });
}
