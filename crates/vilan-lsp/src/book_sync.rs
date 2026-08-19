//! The server's claims about the book, and the book's about the server, held
//! to each other (backlog D18/D19; `proposal/docs-audit-2026-08-18.md` §5).
//!
//! Two gates, test-only:
//!
//! 1. **The keyword-hover deep links resolve.** [`KEYWORD_DOCS`] carries 32
//!    `page.html#anchor` links into the published book. Each page must exist
//!    under `vilan/docs/`, and each anchor must be the id mdBook gives one of
//!    that page's headings. No renderer is required at test time (CI has no
//!    `mdbook`; the docs gate is renderer-independent by design): mdBook's
//!    heading-id algorithm is reimplemented in [`mdbook_heading_ids`], and an
//!    `#[ignore]`d test builds the real book and compares every heading of
//!    every page when `mdbook` is on PATH — the reimplementation is a
//!    compatibility surface (docs-port.md §4 Q3), and that test is its proof.
//!
//! 2. **`appendix/editor.md` stays true.** The page is hand-written prose
//!    about this crate: it quotes code-action titles, setting names and the
//!    capabilities the server does and does not advertise. Each claim is read
//!    out of the page by its own formatting (the quick-fix table's
//!    double-backticked cells, the settings table's backticked names, the
//!    bold feature names) and checked against the thing it describes — the
//!    `title:` literals the server constructs its actions with,
//!    `server_capabilities()`, and `editors/vscode/package.json` — with the
//!    page's shape pinned so an extraction cannot silently come back empty.
//!    D15 found the extension's README promising inlay hints on parameters
//!    that `Document::inlay_hints` never produced; nothing would have caught
//!    the book doing the same.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use tower_lsp::lsp_types::{CodeActionKind, CodeActionProviderCapability, ServerCapabilities};

use crate::document::{BOOK_BASE, KEYWORD_DOCS};
use crate::server_capabilities;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn docs_root() -> PathBuf {
    repo_root().join("vilan/docs")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

// --- mdBook heading ids ------------------------------------------------------

/// The `id` mdBook gives each ATX heading of a page, in document order —
/// reimplemented so the hover links can be checked without a renderer, and
/// proven against a built book by [`mdbook_heading_ids_match_the_built_book`]
/// over every heading of every page (447 at the time of writing, zero
/// mismatches).
///
/// The algorithm is mdBook's `id_from_content` + `unique_id_from_content`:
/// take the heading's rendered TEXT — inline code keeps its characters
/// (`` `Shared<T>` `` → `sharedt`), an HTML tag outside code is dropped, the
/// five HTML entities decode — then keep alphanumerics, `_` and `-`
/// (lowercased), turn each whitespace character into one `-`, and drop every
/// other character. There is no collapsing: `` `if` / `else` `` is `if--else`,
/// and `impl: methods and statics` is `impl-methods-and-statics` (one hyphen
/// — the `:` vanishes, it does not become a `-`). A repeated id gets `-1`,
/// `-2`, … in order of appearance. A trailing `{#custom}` names the id
/// outright. Not modelled: `_emphasis_` (an `_` is kept as a character, which
/// is right for `snake_case` and wrong for underscore emphasis — the book uses
/// neither in a heading; the `#[ignore]`d test is the backstop).
pub(crate) fn mdbook_heading_ids(markdown: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut fence: Option<(char, usize)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        // Fenced code: ``` or ~~~ of any length ≥ 3, closed by the same
        // character at the same length or longer. Headings inside are text.
        if let Some(marker) = trimmed.chars().next().filter(|c| *c == '`' || *c == '~') {
            let run = trimmed.chars().take_while(|c| *c == marker).count();
            if run >= 3 {
                match fence {
                    None => {
                        fence = Some((marker, run));
                        continue;
                    }
                    Some((open, length)) if open == marker && run >= length => {
                        fence = None;
                        continue;
                    }
                    Some(_) => {}
                }
            }
        }
        if fence.is_some() {
            continue;
        }
        let Some(text) = atx_heading_text(line) else {
            continue;
        };
        let id = match custom_id(&text) {
            Some(custom) => custom.to_string(),
            None => normalize_id(&heading_plain_text(&text)),
        };
        let count = seen.entry(id.clone()).or_insert(0);
        ids.push(if *count == 0 {
            id.clone()
        } else {
            format!("{id}-{count}")
        });
        *count += 1;
    }
    ids
}

/// The text of an ATX heading line (`## Title ##` → `Title`), or `None` when
/// the line is not one: up to three spaces of indent, one to six `#`, then a
/// space/tab or the end of the line; an optional closing run of `#` is dropped.
fn atx_heading_text(line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    let mut text = after.trim();
    // A closing sequence: spaces then `#`s, at the very end.
    let without_hashes = text.trim_end_matches('#');
    if without_hashes.len() < text.len()
        && (without_hashes.is_empty() || without_hashes.ends_with([' ', '\t']))
    {
        text = without_hashes.trim_end();
    }
    Some(text.to_string())
}

/// `Title {#custom-id}` → `Some("custom-id")`.
fn custom_id(text: &str) -> Option<&str> {
    let rest = text.strip_suffix('}')?;
    let at = rest.rfind("{#")?;
    Some(&rest[at + 2..])
}

/// The heading's text as the reader sees it: code spans contribute their raw
/// characters (backticks dropped); outside them, an HTML tag is removed, a
/// `[text](url)` / `![alt](url)` contributes its text, and the five HTML
/// entities decode.
fn heading_plain_text(text: &str) -> String {
    let mut plain = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character == '`' {
            let run = chars[index..].iter().take_while(|c| **c == '`').count();
            // The span closes at the next run of exactly `run` backticks; an
            // unclosed run is literal backticks (which normalize drops anyway).
            let mut cursor = index + run;
            let mut closed = None;
            while cursor < chars.len() {
                if chars[cursor] == '`' {
                    let closing = chars[cursor..].iter().take_while(|c| **c == '`').count();
                    if closing == run {
                        closed = Some(cursor);
                        break;
                    }
                    cursor += closing;
                } else {
                    cursor += 1;
                }
            }
            match closed {
                Some(close) => {
                    plain.extend(&chars[index + run..close]);
                    index = close + run;
                }
                None => {
                    plain.extend(&chars[index..index + run]);
                    index += run;
                }
            }
            continue;
        }
        if character == '<' {
            if let Some(close) = chars[index..].iter().position(|c| *c == '>') {
                index += close + 1;
                continue;
            }
        }
        if character == '[' || (character == '!' && chars.get(index + 1) == Some(&'[')) {
            let open = if character == '!' { index + 1 } else { index };
            if let Some(close) = chars[open..].iter().position(|c| *c == ']') {
                let close = open + close;
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = chars[close + 1..].iter().position(|c| *c == ')') {
                        plain.extend(&chars[open + 1..close]);
                        index = close + 1 + end + 1;
                        continue;
                    }
                }
            }
        }
        if character == '&' {
            let rest: String = chars[index..].iter().take(6).collect();
            let decoded = [
                ("&amp;", '&'),
                ("&lt;", '<'),
                ("&gt;", '>'),
                ("&quot;", '"'),
                ("&#39;", '\''),
            ]
            .into_iter()
            .find(|(entity, _)| rest.starts_with(entity));
            if let Some((entity, decoded)) = decoded {
                plain.push(decoded);
                index += entity.chars().count();
                continue;
            }
        }
        plain.push(character);
        index += 1;
    }
    plain
}

/// mdBook's `normalize_id`: alphanumerics, `_` and `-` kept (lowercased),
/// whitespace to `-`, everything else dropped.
fn normalize_id(text: &str) -> String {
    text.chars()
        .filter_map(|character| {
            if character.is_alphanumeric() || character == '_' || character == '-' {
                Some(character.to_ascii_lowercase())
            } else if character.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// The `id` of every `<hN id="…">` in a rendered page, in order.
fn rendered_heading_ids(html: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<h") {
        rest = &rest[at + 2..];
        let mut characters = rest.chars();
        if !characters.next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Some(tail) = rest[1..].strip_prefix(" id=\"")
            && let Some(end) = tail.find('"')
        {
            ids.push(tail[..end].to_string());
            rest = &tail[end..];
        }
    }
    ids
}

/// Every page of the book: `(path relative to vilan/docs without `.md`,
/// markdown)`, `SUMMARY.md` excluded.
fn book_pages() -> Vec<(String, String)> {
    fn walk(directory: &Path, root: &Path, pages: &mut Vec<(String, String)>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
            .map(|entry| entry.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|name| name == "book" || name == "theme")
                {
                    continue;
                }
                walk(&path, root, pages);
            } else if path.extension().is_some_and(|extension| extension == "md")
                && path.file_name().is_some_and(|name| name != "SUMMARY.md")
            {
                let relative = path
                    .strip_prefix(root)
                    .expect("under the docs root")
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                pages.push((relative, read(&path)));
            }
        }
    }
    let root = docs_root();
    let mut pages = Vec::new();
    walk(&root, &root, &mut pages);
    assert!(pages.len() > 50, "the book has {} pages?", pages.len());
    pages
}

// --- D19: the hover links ----------------------------------------------------

#[test]
fn keyword_hover_links_resolve_to_a_heading_in_the_book() {
    let mut headings: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut broken = Vec::new();
    for (keyword, _, link) in KEYWORD_DOCS {
        let Some((page, anchor)) = link.split_once('#') else {
            broken.push(format!("`{keyword}` → {link}: no #anchor"));
            continue;
        };
        let Some(stem) = page.strip_suffix(".html") else {
            broken.push(format!(
                "`{keyword}` → {link}: not a `page.html#anchor` link"
            ));
            continue;
        };
        let source = docs_root().join(format!("{stem}.md"));
        if !source.is_file() {
            broken.push(format!(
                "`{keyword}` → {link}: no page vilan/docs/{stem}.md"
            ));
            continue;
        }
        let ids = headings
            .entry(stem.to_string())
            .or_insert_with(|| mdbook_heading_ids(&read(&source)));
        if !ids.iter().any(|id| id == anchor) {
            broken.push(format!(
                "`{keyword}` → {link}: vilan/docs/{stem}.md has no heading with id `{anchor}` (it has {ids:?})"
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "keyword-hover links into {BOOK_BASE} that do not resolve (KEYWORD_DOCS, document.rs):\n{}",
        broken.join("\n")
    );
}

/// The proof of [`mdbook_heading_ids`]: build the book with the real
/// renderer and compare every heading id of every page. Needs `mdbook` on
/// PATH (`cargo install mdbook`); run with
/// `cargo test -p vilan-lsp book_sync -- --ignored`.
#[test]
#[ignore = "needs `mdbook` on PATH: builds the book and compares every heading id to mdbook_heading_ids"]
fn mdbook_heading_ids_match_the_built_book() {
    let output = std::env::temp_dir().join(format!("vilan-book-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output);
    let build = Command::new("mdbook")
        .arg("build")
        .arg(docs_root())
        .arg("--dest-dir")
        .arg(&output)
        .output()
        .expect("run mdbook (is it on PATH?)");
    assert!(
        build.status.success(),
        "mdbook build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let mut mismatches = Vec::new();
    let mut headings = 0;
    for (stem, markdown) in book_pages() {
        let rendered = output.join(if stem == "README" {
            "index.html".to_string()
        } else {
            format!("{stem}.html")
        });
        let expected = rendered_heading_ids(&read(&rendered));
        let actual = mdbook_heading_ids(&markdown);
        headings += expected.len();
        if expected != actual {
            mismatches.push(format!(
                "{stem}.md:\n  mdbook: {expected:?}\n  ours:   {actual:?}"
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&output);
    assert!(
        headings > 400,
        "only {headings} headings in the built book?"
    );
    assert!(
        mismatches.is_empty(),
        "mdbook_heading_ids disagrees with the built book:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn mdbook_heading_ids_reproduces_the_known_shapes() {
    let page = "\
# Control flow

## `if` / `else`
## impl: methods and statics
## `Shared<T>`: one cell, many holders
## Early return: `ret`
### 6.3 Rule 3 — references are second-class views
## Macros & const
## The CSS `<link>` idiom
## Loops
## Loops
## Loops
```
# not a heading: inside a fence
```
#not-a-heading
## Custom {#named}
";
    assert_eq!(
        mdbook_heading_ids(page),
        [
            "control-flow",
            "if--else",
            "impl-methods-and-statics",
            "sharedt-one-cell-many-holders",
            "early-return-ret",
            "63-rule-3--references-are-second-class-views",
            "macros--const",
            "the-css-link-idiom",
            "loops",
            "loops-1",
            "loops-2",
            "named",
        ]
    );
}

// --- D18: the editor page ----------------------------------------------------

const EDITOR_PAGE: &str = "vilan/docs/appendix/editor.md";

fn editor_page() -> String {
    read(&repo_root().join(EDITOR_PAGE))
}

/// The body of the page's `## {title}` section: the lines after that heading
/// up to the next `## `.
fn section<'a>(page: &'a str, title: &str) -> &'a str {
    let heading = format!("## {title}\n");
    let start = page.find(&heading).unwrap_or_else(|| {
        panic!("{EDITOR_PAGE} has no `## {title}` section — the page's shape changed")
    }) + heading.len();
    let body = &page[start..];
    match body.find("\n## ") {
        Some(end) => &body[..end],
        None => body,
    }
}

/// The data rows of every table in `text`, as trimmed cells: a table is a run
/// of `|`-led lines whose first is the header and second the `|---|`
/// separator; the rest are data.
fn table_data_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut position_in_table = 0;
    for line in text.lines() {
        if !line.starts_with('|') {
            position_in_table = 0;
            continue;
        }
        position_in_table += 1;
        if position_in_table <= 2 {
            continue;
        }
        rows.push(
            line.trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect(),
        );
    }
    rows
}

/// A `title:` string literal as the server writes it, with the constructor it
/// sits in (`QuickFix` or `CodeAction`) — a `format!` template keeps its
/// `{holes}`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TitleLiteral {
    constructor: &'static str,
    template: String,
}

/// Every `title:` string literal in a `QuickFix { … }` (document.rs) or
/// `CodeAction { … }` (main.rs) constructor — the titles the server offers,
/// read from the source that builds them. A constructor whose title is not a
/// literal (`title: fix.title`, the quick-fix relay) is skipped.
fn server_code_action_titles() -> Vec<TitleLiteral> {
    let mut titles = Vec::new();
    for (source, constructor) in [
        (include_str!("document.rs"), "QuickFix"),
        (include_str!("main.rs"), "CodeAction"),
    ] {
        let opener = format!("{constructor} {{");
        for (index, _) in source.match_indices(&opener) {
            let body = struct_literal_body(&source[index + opener.len() - 1..]);
            let Some(at) = body.find("title:") else {
                continue;
            };
            let value = body[at + "title:".len()..].trim_start();
            let literal = value
                .strip_prefix("format!(")
                .map(str::trim_start)
                .unwrap_or(value);
            if let Some(template) = string_literal(literal) {
                titles.push(TitleLiteral {
                    constructor,
                    template,
                });
            }
        }
    }
    titles.sort();
    titles
}

/// The text between the `{` at the start of `source` and its matching `}`,
/// string literals skipped over (their braces are text).
fn struct_literal_body(source: &str) -> &str {
    let bytes = source.as_bytes();
    assert_eq!(
        bytes.first(),
        Some(&b'{'),
        "struct_literal_body starts at a `{{`"
    );
    let mut depth = 0usize;
    let mut in_string = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if in_string => index += 1,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return &source[1..index];
                }
            }
            _ => {}
        }
        index += 1;
    }
    panic!("unbalanced struct literal");
}

/// The contents of the `"…"` literal `source` starts with, unescaped for the
/// one escape a title uses (`\"`), or `None` when it starts with no literal.
fn string_literal(source: &str) -> Option<String> {
    let rest = source.strip_prefix('"')?;
    let mut literal = String::new();
    let mut characters = rest.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => literal.push(characters.next()?),
            '"' => return Some(literal),
            other => literal.push(other),
        }
    }
    None
}

/// Whether `title` instantiates `template`: the template's `{holes}` match
/// any text, everything else matches verbatim, in order.
fn instantiates(template: &str, title: &str) -> bool {
    let mut segments = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        segments.push(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            return false;
        };
        rest = &rest[open + close + 1..];
    }
    segments.push(rest);
    let (first, middle) = segments.split_first().expect("at least one segment");
    let Some(mut remaining) = title.strip_prefix(first) else {
        return false;
    };
    let Some((last, middle)) = middle.split_last() else {
        return remaining.is_empty();
    };
    for segment in middle {
        let Some(at) = remaining.find(segment) else {
            return false;
        };
        remaining = &remaining[at + segment.len()..];
    }
    remaining.ends_with(last)
}

#[test]
fn editor_page_code_action_titles_are_the_servers() {
    let page = editor_page();
    let quick_fixes = section(&page, "Quick fixes");
    // The page's titles: the quick-fix table's double-backticked cells and
    // the source-action table's bold names, in that section.
    let mut page_quick_fixes = Vec::new();
    let mut page_source_actions = Vec::new();
    for row in table_data_rows(quick_fixes) {
        let cell = row.first().cloned().unwrap_or_default();
        if let Some(inner) = cell
            .strip_prefix("``")
            .and_then(|cell| cell.strip_suffix("``"))
        {
            page_quick_fixes.push(inner.trim().to_string());
        } else if let Some(inner) = cell
            .strip_prefix("**")
            .and_then(|cell| cell.strip_suffix("**"))
        {
            page_source_actions.push(inner.to_string());
        } else {
            panic!(
                "{EDITOR_PAGE}: a Quick fixes row whose first cell is neither ``title`` nor **title**: {cell:?}"
            );
        }
    }
    let server = server_code_action_titles();
    let server_quick_fixes: Vec<&TitleLiteral> = server
        .iter()
        .filter(|title| title.constructor == "QuickFix")
        .collect();
    let server_source_actions: Vec<&TitleLiteral> = server
        .iter()
        .filter(|title| title.constructor == "CodeAction")
        .collect();
    // Shape: both sides non-empty, and the same number of kinds.
    assert!(
        server_quick_fixes.len() >= 4 && server_source_actions.len() >= 2,
        "the source scan found {server:?} — did `QuickFix {{ title: … }}` / `CodeAction {{ title: … }}` change shape?"
    );
    assert_eq!(
        page_quick_fixes.len(),
        server_quick_fixes.len(),
        "{EDITOR_PAGE} documents {page_quick_fixes:?} quick fixes; document.rs constructs {server_quick_fixes:?}"
    );
    // The section opens by counting them ("Four, each attached to the
    // diagnostic that earns it") — the count is a claim too.
    let counted = quick_fixes
        .split_once(", each attached to the diagnostic")
        .map(|(before, _)| before.trim())
        .unwrap_or_else(|| {
            panic!("{EDITOR_PAGE}'s Quick fixes section no longer opens with a count — update this pin with the page")
        });
    let number_words = [
        "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
    ];
    assert_eq!(
        number_words
            .iter()
            .position(|word| *word == counted)
            .map(|index| index + 1),
        Some(server_quick_fixes.len()),
        "{EDITOR_PAGE} counts {counted:?} quick fixes; document.rs constructs {}",
        server_quick_fixes.len()
    );
    assert_eq!(
        page_source_actions.len(),
        server_source_actions.len(),
        "{EDITOR_PAGE} documents {page_source_actions:?} source actions; main.rs constructs {server_source_actions:?}"
    );
    // Every documented title instantiates a server template, and every
    // template is documented.
    for (documented, produced) in [
        (&page_quick_fixes, &server_quick_fixes),
        (&page_source_actions, &server_source_actions),
    ] {
        for title in documented {
            assert!(
                produced
                    .iter()
                    .any(|literal| instantiates(&literal.template, title)),
                "{EDITOR_PAGE} quotes the code action {title:?}, which the server does not produce (its titles: {:?})",
                produced
                    .iter()
                    .map(|literal| &literal.template)
                    .collect::<Vec<_>>()
            );
        }
        for literal in produced {
            assert!(
                documented
                    .iter()
                    .any(|title| instantiates(&literal.template, title)),
                "the server produces the code action {:?}, which {EDITOR_PAGE} does not document (it quotes {documented:?})",
                literal.template
            );
        }
    }
}

/// The page's claims about what the server advertises: the phrase the page
/// uses (pinned to appear verbatim, whitespace flattened), whether it claims
/// the capability present or absent, and the predicate over
/// `server_capabilities()` it is a claim about. "What it does not have" names
/// the absent providers by their LSP feature names (`type hierarchy` has no
/// field in this lsp-types at all, so it cannot be advertised by construction).
type CapabilityClaim = (&'static str, bool, fn(&ServerCapabilities) -> bool);
const CAPABILITY_CLAIMS: &[CapabilityClaim] = &[
    ("**Diagnostics**, published", true, |c| {
        c.text_document_sync.is_some()
    }),
    ("**Hover**", true, |c| c.hover_provider.is_some()),
    ("**Inlay hints**", true, |c| c.inlay_hint_provider.is_some()),
    ("**Semantic highlighting**", true, |c| {
        c.semantic_tokens_provider.is_some()
    }),
    ("**Go to definition**", true, |c| {
        c.definition_provider.is_some()
    }),
    ("**find references**", true, |c| {
        c.references_provider.is_some()
    }),
    ("**rename**", true, |c| c.rename_provider.is_some()),
    ("**document outline**", true, |c| {
        c.document_symbol_provider.is_some()
    }),
    ("**Formatting**", true, |c| {
        c.document_formatting_provider.is_some()
    }),
    ("there is no range or on-type formatting", false, |c| {
        c.document_range_formatting_provider.is_some()
            || c.document_on_type_formatting_provider.is_some()
    }),
    ("**Linked editing**", true, |c| {
        c.linked_editing_range_provider.is_some()
    }),
    ("## Completion", true, |c| c.completion_provider.is_some()),
    ("**After `.`**", true, |c| {
        completion_triggers(c).contains(".")
    }),
    ("**After `::`**", true, |c| {
        completion_triggers(c).contains(":")
    }),
    ("## Quick fixes", true, |c| {
        code_action_kinds(c).contains(&CodeActionKind::QUICKFIX)
    }),
    ("**Organize Imports**", true, |c| {
        code_action_kinds(c).contains(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS)
    }),
    ("no signature-help popup", false, |c| {
        c.signature_help_provider.is_some()
    }),
    ("no folding ranges", false, |c| {
        c.folding_range_provider.is_some()
    }),
    ("no workspace-wide symbol search", false, |c| {
        c.workspace_symbol_provider.is_some()
    }),
    ("no document-highlight-on-cursor", false, |c| {
        c.document_highlight_provider.is_some()
    }),
    ("no code lens", false, |c| c.code_lens_provider.is_some()),
    ("no call or type hierarchy", false, |c| {
        c.call_hierarchy_provider.is_some()
    }),
    (
        "no go-to-type-definition / implementation / declaration",
        false,
        |c| {
            c.type_definition_provider.is_some()
                || c.implementation_provider.is_some()
                || c.declaration_provider.is_some()
        },
    ),
    ("no pull diagnostics", false, |c| {
        c.diagnostic_provider.is_some()
    }),
];

/// The page with its line wrapping undone — one space between words — so a
/// phrase pin does not depend on where a sentence breaks.
fn flattened(page: &str) -> String {
    page.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn completion_triggers(capabilities: &ServerCapabilities) -> BTreeSet<String> {
    capabilities
        .completion_provider
        .as_ref()
        .and_then(|completion| completion.trigger_characters.clone())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn code_action_kinds(capabilities: &ServerCapabilities) -> Vec<CodeActionKind> {
    match &capabilities.code_action_provider {
        Some(CodeActionProviderCapability::Options(options)) => {
            options.code_action_kinds.clone().unwrap_or_default()
        }
        Some(CodeActionProviderCapability::Simple(_)) | None => Vec::new(),
    }
}

#[test]
fn editor_page_capabilities_are_the_servers() {
    let page = flattened(&editor_page());
    let capabilities = server_capabilities();
    let mut wrong = Vec::new();
    for (phrase, claimed, advertised) in CAPABILITY_CLAIMS {
        assert!(
            page.contains(phrase),
            "{EDITOR_PAGE} no longer says {phrase:?} — update CAPABILITY_CLAIMS with the page"
        );
        if advertised(&capabilities) != *claimed {
            wrong.push(format!(
                "the page says {phrase:?} but the server {} that capability",
                if *claimed {
                    "does not advertise"
                } else {
                    "advertises"
                }
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{EDITOR_PAGE} disagrees with server_capabilities():\n{}",
        wrong.join("\n")
    );
}

fn extension_manifest() -> serde_json::Value {
    let path = repo_root().join("editors/vscode/package.json");
    serde_json::from_str(&read(&path)).expect("editors/vscode/package.json is JSON")
}

#[test]
fn editor_page_settings_are_the_extensions() {
    let page = editor_page();
    let manifest = extension_manifest();
    let properties = manifest
        .pointer("/contributes/configuration/properties")
        .and_then(serde_json::Value::as_object)
        .expect("package.json contributes.configuration.properties");
    // The page's settings table: `| `name` | default | … |`.
    let mut documented: BTreeMap<String, String> = BTreeMap::new();
    for row in table_data_rows(section(&page, "Settings")) {
        let name = row.first().map(|cell| cell.trim_matches('`')).unwrap_or("");
        let default = row.get(1).cloned().unwrap_or_default();
        documented.insert(name.to_string(), default);
    }
    assert!(
        documented.len() >= 6,
        "{EDITOR_PAGE}'s Settings table has {documented:?} — did its shape change?"
    );
    let declared: BTreeSet<&String> = properties.keys().collect();
    assert_eq!(
        documented.keys().collect::<BTreeSet<_>>(),
        declared,
        "{EDITOR_PAGE}'s Settings table (left) and package.json's contributes.configuration (right) name different settings"
    );
    // A documented default that is a literal (`true`, `false`, `full`) is the
    // manifest's; `—` documents "no path", which is the manifest's empty or
    // discovery-sentinel default.
    for (name, default) in &documented {
        let manifest_default = &properties[name]["default"];
        match default.trim_matches('`') {
            "—" => assert!(
                manifest_default
                    .as_str()
                    .is_some_and(|path| path.is_empty() || path == "vilan-lsp"),
                "{name}: the page documents no default, package.json has {manifest_default}"
            ),
            literal => assert_eq!(
                manifest_default.to_string().trim_matches('"'),
                literal,
                "{name}: the page documents the default `{literal}`, package.json has {manifest_default}"
            ),
        }
    }
    // The command the page names for the palette, `Category: Title`.
    let commands: Vec<String> = manifest["contributes"]["commands"]
        .as_array()
        .expect("package.json contributes.commands")
        .iter()
        .map(|command| {
            format!(
                "{}: {}",
                command["category"].as_str().unwrap_or(""),
                command["title"].as_str().unwrap_or("")
            )
        })
        .collect();
    assert!(
        flattened(&page).contains("**Vilan: Restart Language Server**"),
        "{EDITOR_PAGE} no longer names the restart command in bold — update this check with the page"
    );
    assert!(
        commands.contains(&"Vilan: Restart Language Server".to_string()),
        "package.json's commands are {commands:?}; the page promises `Vilan: Restart Language Server`"
    );
    // And the book the hovers link into is the book the listing links to.
    assert_eq!(
        manifest["homepage"].as_str(),
        Some(BOOK_BASE),
        "the keyword hovers (document.rs BOOK_BASE) and the marketplace listing (package.json homepage) point at different books"
    );
}
