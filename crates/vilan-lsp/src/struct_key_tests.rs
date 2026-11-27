//! B711 (`struct-keys.md`): a file using struct keys in the editor. S1a pinned
//! that nothing crashes or answers nonsense on `Task.` / `Task.title`, and that
//! an edit to a key literal is diagnosed and cleared through the editing path —
//! the live buffer's ranged edit, then the analysis of what it holds. S1c gives
//! keys their own support: completion of a key's next step, hover and go to
//! definition on a step, a field's references and rename through its keys, and
//! a step's colour.

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

/// The labels a completion at `offset` answers, in order.
fn labels(document: &Document, offset: usize) -> Vec<String> {
    document
        .completion(offset)
        .into_iter()
        .map(|candidate| candidate.label)
        .collect()
}

/// `SOURCE` with `let k = Task.title;` replaced by `let k = <typed>;`, and the
/// offset of the end of `<typed>`.
fn typing(typed: &str) -> (Document, usize) {
    let text = SOURCE.replace("let k = Task.title;", &format!("let k = {typed};"));
    let at = text.find(&format!("{typed};")).expect("the typed key") + typed.len();
    (analyzed(&text), at)
}

/// S1c: after `Task.` — the fields of `Task`, each with its key's type as
/// detail, and nothing else (a type name has no methods to call).
#[test]
fn keys_completion_after_a_struct_type_offers_its_fields_as_keys() {
    on_big_stack(|| {
        let (document, at) = typing("Task.");
        let completed = document.completion(at);
        let rows: Vec<(String, Option<String>)> = completed
            .into_iter()
            .map(|candidate| (candidate.label, candidate.detail))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("title".to_string(), Some("Key<Task, str>".to_string())),
                ("meta".to_string(), Some("Key<Task, Meta>".to_string())),
            ],
        );
    });
}

/// S1c: after `Task.meta.` — the path continues, so `Meta`'s fields come
/// first, still keys over `Task`; then the key value's own methods.
#[test]
fn keys_completion_after_a_path_offers_the_next_fields_then_the_key_methods() {
    on_big_stack(|| {
        let (document, at) = typing("Task.meta.");
        let completed = document.completion(at);
        assert_eq!(completed[0].label, "owner");
        assert_eq!(completed[0].detail.as_deref(), Some("Key<Task, str>"));
        let labels: Vec<&str> = completed
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect();
        for method in ["then", "name", "path", "starts_with"] {
            assert!(labels.contains(&method), "{method} in {labels:?}");
        }
        // The internal `erased` field is no member outside std.
        assert!(!labels.contains(&"erased"), "{labels:?}");
    });
}

/// S1c: past a field whose type has no fields a key can name, nothing
/// continues the path: only the key's methods.
#[test]
fn keys_completion_past_a_leaf_field_offers_only_the_key_methods() {
    on_big_stack(|| {
        let (document, at) = typing("Task.title.");
        let labels = labels(&document, at);
        assert!(labels.contains(&"then".to_string()), "{labels:?}");
        assert!(
            !labels.contains(&"len".to_string()),
            "a key is no `str`: {labels:?}"
        );
    });
}

/// S1c, R9: where a VALUE of the name is in scope, the value wins — its own
/// members, not keys.
#[test]
fn keys_completion_on_a_value_named_like_a_type_is_the_values() {
    on_big_stack(|| {
        let text = SOURCE.replace(
            "let k = Task.title;",
            "let Task = Meta { owner = \"x\" };\n\tlet k = Task.;",
        );
        let at = text.find("Task.;").expect("the typed member") + "Task.".len();
        let document = analyzed(&text);
        let completed = document.completion(at);
        let owner = completed
            .iter()
            .find(|candidate| candidate.label == "owner")
            .expect("the value's field");
        assert_eq!(owner.detail.as_deref(), Some("str"));
        assert!(!completed.iter().any(|candidate| candidate.label == "title"));
    });
}

/// S1c: a root written through its module (`model::Task.`).
#[test]
fn keys_completion_after_a_module_qualified_root() {
    on_big_stack(|| {
        let text = "mod model {\n\texport struct Task {\n\t\ttitle: str,\n\t\tdone: bool,\n\t}\n}\n\n\
             fun main() {\n\tlet k = model::Task.title;\n\tprint(k.name());\n}\n";
        let typed = text.replace("model::Task.title;", "model::Task.;");
        let at = typed.find("Task.;").expect("the typed key") + "Task.".len();
        let document = analyzed(&typed);
        let rows: Vec<(String, Option<String>)> = document
            .completion(at)
            .into_iter()
            .map(|candidate| (candidate.label, candidate.detail))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("title".to_string(), Some("Key<Task, str>".to_string())),
                ("done".to_string(), Some("Key<Task, bool>".to_string())),
            ],
        );
    });
}

/// S1c: a step hovers as the key it spells, its type, then the field's
/// declaration and its `///`.
#[test]
fn keys_hover_on_a_step_shows_the_key_and_the_field() {
    on_big_stack(|| {
        let text = SOURCE.replace("\towner: str,", "\t/// Who owns the task.\n\towner: str,");
        let document = analyzed(&text);
        let step = text.find("Task.meta.owner").expect("the path") + "Task.meta.".len() + 1;
        let hover = document.hover(step).expect("a hover on the step");
        assert!(hover.contains("Task.meta.owner: Key<Task, str>"), "{hover}");
        assert!(hover.contains("struct Meta"), "{hover}");
        assert!(hover.contains("owner: str"), "{hover}");
        assert!(hover.contains("Who owns the task."), "{hover}");
        let middle = text.find("Task.meta.owner").expect("the path") + "Task.".len() + 1;
        let hover = document.hover(middle).expect("a hover on the middle step");
        assert!(hover.contains("Task.meta: Key<Task, Meta>"), "{hover}");
    });
}

/// S1c: go to definition on a step lands on the field, on the root on the
/// struct.
#[test]
fn keys_definition_of_a_step_is_its_field_and_of_the_root_its_struct() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let path = SOURCE.find("Task.meta.owner").expect("the path");
        let (_, owner) = document
            .definition(path + "Task.meta.".len() + 1)
            .expect("the field");
        assert_eq!(owner.start, SOURCE.find("owner: str").expect("the field"));
        let (_, meta) = document
            .definition(path + "Task.".len() + 1)
            .expect("the field");
        assert_eq!(meta.start, SOURCE.find("meta: Meta").expect("the field"));
        // The root lands on the struct's declaration, as a struct named in
        // value position does.
        let (_, task) = document.definition(path + 1).expect("the struct");
        assert_eq!(task.start, SOURCE.find("struct Task").expect("the struct"));
    });
}

/// S1c: a field's references list the key literals that name it, and a rename
/// of the field rewrites every one of them, at every step of a path.
#[test]
fn keys_renaming_a_field_rewrites_its_key_literals() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let declaration = SOURCE.find("owner: str").expect("the field");
        let path_step = SOURCE.find("Task.meta.owner").expect("the path") + "Task.meta.".len();
        let starts: Vec<usize> = document
            .references(declaration)
            .into_iter()
            .map(|(_, span)| span.start)
            .collect();
        assert!(starts.contains(&path_step), "{starts:?}");
        let edits = document
            .rename_edits(declaration, "holder")
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        let mut renamed = SOURCE.to_string();
        let mut spans: Vec<(usize, usize)> = edits
            .iter()
            .map(|(_, span, _)| (span.start, span.end))
            .collect();
        spans.sort_unstable();
        for (start, end) in spans.into_iter().rev() {
            renamed.replace_range(start..end, "holder");
        }
        assert!(renamed.contains("Task.meta.holder"), "{renamed}");
        let after = analyzed(&renamed);
        assert!(after.diagnostics.is_empty(), "{:?}", messages(&after));
        // And `title`, through its held literal.
        let title = SOURCE.find("title: str").expect("the field");
        let edits = document
            .rename_edits(title, "heading")
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        let literal = SOURCE.find("Task.title;").expect("the literal") + "Task.".len();
        assert!(
            edits.iter().any(|(_, span, _)| span.start == literal),
            "{edits:?}"
        );
    });
}

/// S1c: a path's MIDDLE step is renamed with its field, and its root with
/// its struct, and the file checks clean after each.
#[test]
fn keys_renaming_a_middle_step_and_the_root_rewrites_the_path() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let apply = |at: usize, to: &str| {
            let edits = document
                .rename_edits(at, to)
                .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
            let mut spans: Vec<(usize, usize)> = edits
                .iter()
                .map(|(_, span, _)| (span.start, span.end))
                .collect();
            spans.sort_unstable();
            let mut renamed = SOURCE.to_string();
            for (start, end) in spans.into_iter().rev() {
                renamed.replace_range(start..end, to);
            }
            renamed
        };
        let middle = apply(SOURCE.find("meta: Meta").expect("the field"), "info");
        assert!(middle.contains("Task.info.owner"), "{middle}");
        let after = analyzed(&middle);
        assert!(after.diagnostics.is_empty(), "{:?}", messages(&after));
        let root = apply(
            SOURCE.find("struct Task").expect("the struct") + "struct ".len(),
            "Job",
        );
        assert!(root.contains("Job.title;"), "{root}");
        assert!(root.contains("Job.meta.owner"), "{root}");
        let after = analyzed(&root);
        assert!(after.diagnostics.is_empty(), "{:?}", messages(&after));
    });
}

/// S1c: a key's steps colour as fields, its root as a type — the existing
/// classification, read off the member spans and the root's reference.
#[test]
fn keys_steps_colour_as_properties_and_the_root_as_a_struct() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let tokens = document.semantic_tokens();
        let path = SOURCE.find("Task.meta.owner").expect("the path");
        let kind_at = |start: usize| {
            tokens
                .iter()
                .find(|(span, _, _)| span.start == start)
                .map(|(_, kind, _)| format!("{kind:?}"))
        };
        assert_eq!(kind_at(path).as_deref(), Some("Struct"));
        assert_eq!(kind_at(path + "Task.".len()).as_deref(), Some("Property"));
        assert_eq!(
            kind_at(path + "Task.meta.".len()).as_deref(),
            Some("Property")
        );
    });
}

/// S1c: a tuple key's members show in completion (`index`), as any value's.
#[test]
fn keys_a_tuple_keys_members_complete() {
    on_big_stack(|| {
        let text = "import std::tuple::Tuple;\n\nfun main() {\n\tlet (a, b) = (1, 2).keys();\n\tprint(a.index());\n}\n";
        let typed = text.replace("a.index()", "a.");
        let at = typed.find("a.)").expect("the member") + "a.".len();
        let document = analyzed(&typed);
        let labels = labels(&document, at);
        assert!(labels.contains(&"index".to_string()), "{labels:?}");
    });
}

// --- M211: `std::key` loads on demand ----------------------------------------

/// A file with a struct and no key.
const KEYLESS: &str = "struct Task {\n\ttitle: str,\n}\n\n\
     fun main() {\n\tlet t = Task { title = \"a\" };\n\tprint(t.title);\n}\n";

/// Whether the document's analysis loaded `std::key`.
fn loads_key(document: &Document) -> bool {
    document
        .program
        .as_ref()
        .is_some_and(|program| program.key_struct_id.is_some())
}

/// M211: an open file with no key loads no `std::key`; the keystroke that
/// writes the first key literal brings it in with nothing stale left behind,
/// and the one that takes the literal out again lands on the keyless world
/// (the module is dropped, not kept) — clean at every step.
#[test]
fn m211_the_first_key_literal_loads_std_key_and_removing_it_unloads_it() {
    on_big_stack(|| {
        let mut document = analyzed(KEYLESS);
        assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
        assert!(!loads_key(&document));
        let at = KEYLESS.find("t.title)").expect("the read");
        let keyed = edited(&mut document, at + 1, at + 1, "[Task.title]");
        let keyed = {
            let mut keyed = keyed;
            let read = keyed.text.find("[Task.title].title").expect("the edit");
            edited(
                &mut keyed,
                read + "[Task.title]".len(),
                read + "[Task.title].title".len(),
                "",
            )
        };
        assert!(
            keyed.text.contains("print(t[Task.title]);"),
            "{}",
            keyed.text
        );
        assert!(keyed.diagnostics.is_empty(), "{:?}", messages(&keyed));
        assert!(loads_key(&keyed));
        let mut keyed = keyed;
        let read = keyed.text.find("[Task.title]").expect("the key");
        let back = edited(&mut keyed, read, read + "[Task.title]".len(), ".title");
        assert_eq!(back.text, KEYLESS);
        assert!(back.diagnostics.is_empty(), "{:?}", messages(&back));
        assert!(!loads_key(&back));
    });
}

/// M211: the first `Key` written in a signature, then a `[field_syntax]`
/// method, each brings the module in, clean.
#[test]
fn m211_the_first_key_in_a_signature_or_a_field_syntax_method_loads_std_key() {
    on_big_stack(|| {
        let mut document = analyzed(KEYLESS);
        assert!(!loads_key(&document));
        let at = KEYLESS.find("fun main()").expect("main");
        let signature = "fun read(t: Task, k: Key<Task, str>): str {\n\tt[k]\n}\n\n";
        let signed = edited(&mut document, at, at, signature);
        assert!(signed.diagnostics.is_empty(), "{:?}", messages(&signed));
        assert!(loads_key(&signed));
        let mut document = analyzed(KEYLESS);
        let door = "struct Box {\n\ttask: Task,\n}\n\nimpl Box {\n\t[field_syntax]\n\t\
                    fun at<U>(self, key: Key<Task, U>): U {\n\t\tself.task[key]\n\t}\n}\n\n";
        let doored = edited(&mut document, at, at, door);
        assert!(doored.diagnostics.is_empty(), "{:?}", messages(&doored));
        assert!(loads_key(&doored));
    });
}

/// M211: completion offers the ambient `Key` in a file that has not used it
/// — its module not loaded — exactly as it does once the module is.
#[test]
fn m211_completion_offers_key_before_any_use() {
    on_big_stack(|| {
        let text = KEYLESS.replace("\tprint(t.title);\n", "\tlet k: Ke\n");
        let document = analyzed(&text);
        assert!(!loads_key(&document));
        let at = text.find("let k: Ke").expect("the typed name") + "let k: Ke".len();
        let offered = document.completion(at);
        let key = offered
            .iter()
            .find(|candidate| candidate.label == "Key")
            .unwrap_or_else(|| {
                panic!(
                    "{:?}",
                    offered
                        .iter()
                        .map(|candidate| &candidate.label)
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(key.kind, vilan_ide::CompletionKind::Struct);
        let keyed = analyzed(&SOURCE.replace("\tprint(t[k]);\n", "\tlet j: Ke\n"));
        assert!(loads_key(&keyed));
        let at = keyed.text.find("let j: Ke").expect("the typed name") + "let j: Ke".len();
        let bound = keyed
            .completion(at)
            .into_iter()
            .find(|candidate| candidate.label == "Key")
            .expect("the bound `Key`");
        assert_eq!(bound.kind, key.kind);
        assert_eq!(bound.detail, key.detail);
    });
}
