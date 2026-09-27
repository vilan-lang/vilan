//! The docs compile gate (proposal/documentation.md §4): every fenced code
//! block in `vilan/docs/**/*.md` is extracted and compiled through the real
//! pipeline, so a std or language change that breaks a documented example
//! fails the suite until the doc is updated.
//!
//! Fence tags:
//! - ```vilan           — complete program, node target
//! - ```vilan,browser   — complete program, browser target
//! - ```vilan,norun     — compiles (node) but isn't runnable standalone
//! - ```vilan,fragment  — NOT compiled (a signature, a diff, a deliberate error)
//!
//! Failures report `file — nearest heading` so a broken example is a one-jump
//! fix.
//!
//! Two prose gates ride here too, for the same reason: a list a reader trusts,
//! held to the compiler's own tables rather than to a lane's diligence. The
//! reserved-word lists (N64, N87) are held to `lexing::KEYWORDS`, and std's doc
//! comments are held to the names std still declares (N88) — in a library whose
//! doc comment IS its documentation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

fn docs_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/docs")
}

/// One extracted example: where it came from, what it contains, and how the
/// fence asked for it to be compiled.
struct Example {
    file: PathBuf,
    heading: String,
    line: usize,
    source: String,
    platform: Platform,
}

fn collect_markdown_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            // The rendered-site build output is not content.
            if path.file_name().is_some_and(|name| name == "book") {
                continue;
            }
            collect_markdown_files(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            into.push(path);
        }
    }
}

/// A fenced block in progress: how it was tagged, where it opened, and — the
/// crux of the indent handling — the column its opening fence sat at.
struct OpenFence {
    platform: Platform,
    compile: bool,
    opened_at: usize,
    indent: usize,
    body: String,
}

/// The count of leading ASCII spaces on `line` — the fence-indent measure. Tabs
/// are deliberately NOT counted: our pages indent fenced blocks with spaces
/// (mdBook renders through pulldown-cmark, a CommonMark implementation, and the
/// book uses space indentation throughout), so a tab-indented fence is out of
/// scope and simply reads as column 0.
fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

/// Strip up to `indent` leading spaces from `line` — CommonMark's fenced-code
/// dedent (§4.5): a line indented at least `indent` loses exactly `indent`
/// spaces; a line indented less loses only what it has (never more). Slices the
/// input, no allocation.
fn dedent(line: &str, indent: usize) -> &str {
    &line[leading_spaces(line).min(indent)..]
}

/// Pull the compilable examples out of one markdown file, tracking the nearest
/// heading for failure reports.
///
/// NOTE — keep in sync with `parse_differential.rs`'s `collect_doc_examples`,
/// which carries a copy of this fence logic (test targets cannot import one
/// another). The two MUST agree on which fences are examples and on their
/// dedented bodies; a change to the fence rules here belongs there too.
fn extract_examples(path: &Path) -> Vec<Example> {
    let text = std::fs::read_to_string(path).expect("read doc file");
    extract_examples_from(&text, path)
}

/// The pure core of [`extract_examples`], over in-memory text so the fence rules
/// — indent tracking, same-indent close, and dedent — can be unit-tested without
/// touching the filesystem.
fn extract_examples_from(text: &str, path: &Path) -> Vec<Example> {
    let mut examples = Vec::new();
    let mut heading = String::from("(top)");
    let mut fence: Option<OpenFence> = None;
    for (index, line) in text.lines().enumerate() {
        match &mut fence {
            Some(open) => {
                // CommonMark closes a fenced block on a fence line at the SAME
                // indentation as its opener; a ``` at any other indent is body
                // (e.g. a fence-like line inside the code). This is what stops an
                // indented fence from running past its close into the prose. The
                // indent check is first so the slice below is always in bounds.
                if leading_spaces(line) == open.indent && line[open.indent..].trim_end() == "```" {
                    if open.compile {
                        examples.push(Example {
                            file: path.to_path_buf(),
                            heading: heading.clone(),
                            line: open.opened_at + 1,
                            source: std::mem::take(&mut open.body),
                            platform: open.platform,
                        });
                    }
                    fence = None;
                } else {
                    open.body.push_str(dedent(line, open.indent));
                    open.body.push('\n');
                }
            }
            None => {
                let indent = leading_spaces(line);
                if let Some(info) = line[indent..].strip_prefix("```") {
                    let info = info.trim();
                    let (platform, compile) = match info {
                        "vilan" | "vilan,norun" => (Platform::default(), true),
                        "vilan,browser" => (Platform::Browser, true),
                        "vilan,fragment" => (Platform::default(), false),
                        // Non-vilan fences (sh, toml, text, js …) are prose.
                        _ => (Platform::default(), false),
                    };
                    fence = Some(OpenFence {
                        platform,
                        compile,
                        opened_at: index,
                        indent,
                        body: String::new(),
                    });
                } else if let Some(title) = line.strip_prefix('#') {
                    heading = title.trim_start_matches('#').trim().to_string();
                }
            }
        }
    }
    assert!(fence.is_none(), "unclosed code fence in {}", path.display());
    examples
}

/// Compile one example through the full pipeline on a large-stack worker
/// (mirroring the CLI and the `inference` suite); a panic becomes an error rather
/// than aborting the suite.
fn compile(source: &str, platform: Platform) -> Result<(), Vec<String>> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let leaked: &'static str = Box::leak(source.into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    Path::new("."),
                    Path::new("doc-example.vl"),
                    Some(platform),
                    &Workspace::default(),
                );
                match program {
                    Some(program) if errors.is_empty() => {
                        transform(&program, &BuildOptions::default())
                            .map(|_| ())
                            .map_err(|error| vec![error.msg])
                    }
                    _ => Err(errors.into_iter().map(|error| error.msg).collect()),
                }
            }))
            .unwrap_or_else(|_| Err(vec!["compiler panicked".to_string()]))
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| {
            Err(vec![
                "compiler thread aborted (likely a stack overflow)".to_string(),
            ])
        })
}

#[test]
fn every_doc_example_compiles() {
    let mut files = Vec::new();
    collect_markdown_files(&docs_root(), &mut files);
    assert!(
        !files.is_empty(),
        "no markdown files under {} — docs missing?",
        docs_root().display()
    );
    // The repo-root README carries examples too; hold it to the same gate.
    let readme = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    if readme.is_file() {
        files.push(readme);
    }
    let examples: Vec<Example> = files
        .iter()
        .flat_map(|file| extract_examples(file))
        .collect();
    let compiled = examples.len();
    // Every example is an independent compile, so run them in parallel chunks
    // (corpus.rs's shape). Chunks preserve extraction order and workers join
    // in spawn order, so a failure report reads the same as the serial loop's.
    let failures: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = examples
            .chunks(examples.len().div_ceil(8).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for example in chunk {
                        if let Err(errors) = compile(&example.source, example.platform) {
                            failures.push(format!(
                                "{}:{} — {} — {:?}",
                                example.file.display(),
                                example.line,
                                example.heading,
                                errors
                            ));
                        }
                    }
                    failures
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("compile worker panicked"))
            .collect()
    });
    assert!(
        failures.is_empty(),
        "{} of {} doc examples failed to compile:\n{}",
        failures.len(),
        compiled,
        failures.join("\n")
    );
    // The gate is only meaningful while examples exist.
    assert!(compiled > 0, "no compilable examples found in docs/");
}

/// The rendered site's sidebar (proposal/docs-site.md §5): every docs page
/// appears in SUMMARY.md, and every SUMMARY.md entry points at a real file —
/// a page added without nav wiring, or moved without fixing it, fails here.
#[test]
fn the_sidebar_covers_every_page() {
    let root = docs_root();
    let summary_path = root.join("SUMMARY.md");
    let summary = std::fs::read_to_string(&summary_path).expect("read SUMMARY.md");

    let mut listed = std::collections::BTreeSet::new();
    for capture in summary.split("](").skip(1) {
        let Some(target) = capture.split(')').next() else {
            continue;
        };
        if target.ends_with(".md") {
            listed.insert(target.to_string());
        }
    }

    let mut files = Vec::new();
    collect_markdown_files(&root, &mut files);
    let mut missing_from_summary = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(&root)
            .expect("docs file under docs root")
            .to_string_lossy()
            .replace('\\', "/");
        // The build output and the sidebar itself are not pages.
        if relative.starts_with("book/") || relative == "SUMMARY.md" {
            continue;
        }
        if !listed.remove(&relative) {
            missing_from_summary.push(relative);
        }
    }
    assert!(
        missing_from_summary.is_empty(),
        "pages missing from SUMMARY.md (add them to the sidebar): {missing_from_summary:?}"
    );
    assert!(
        listed.is_empty(),
        "SUMMARY.md entries with no file behind them: {listed:?}"
    );
}

/// The tour's reserved-word list is the LEXER's, held both ways (tracker N64).
///
/// `own` was a keyword documented nowhere in the tour and cost a lane a debug
/// cycle on a fixture that used it as a name — which is the failure a reader of
/// the tour is set up for, since the tour teaches by example and an example
/// never shows the word you cannot use. So the tour names them, and the list is
/// gated rather than trusted: a hand-copied word list is one commit from being
/// wrong (the three-place rule's own history, and N60's "nine" one file over).
///
/// `grammar_sync.rs` holds the machine-read grammars to the same table; this is
/// the reader-facing list, and N87 added the spec's two below.
#[test]
fn the_tours_reserved_words_are_the_lexers_keywords() {
    assert_reserved_words_match(
        &reserved_word_fence("tour/values-and-types.md", "## Reserved words"),
        "the tour's reserved-word list",
        "add it to `## Reserved words` in `vilan/docs/tour/values-and-types.md`, \
         alphabetically.",
    );
}

/// The `text` fence under `heading` in a docs page, as a set of words.
///
/// Every reserved-word list in the tree is written this way — one fence, words
/// separated by whitespace — so the three gates below read them the same way
/// and a list that stops being a fence reds loudly rather than going empty.
fn reserved_word_fence(relative: &str, heading: &str) -> BTreeSet<String> {
    let page = docs_root().join(relative);
    let text = std::fs::read_to_string(&page)
        .unwrap_or_else(|error| panic!("{}: {error}", page.display()));
    let after = text
        .split_once(heading)
        .unwrap_or_else(|| panic!("`{heading}` is gone from {}", page.display()))
        .1;
    let fence = after
        .split_once("```text\n")
        .unwrap_or_else(|| panic!("{heading} opens no `text` fence in {relative}"))
        .1
        .split_once("```")
        .unwrap_or_else(|| panic!("{heading}'s fence does not close in {relative}"))
        .0;
    fence.split_whitespace().map(str::to_string).collect()
}

/// The lexer's keyword table, as a set of words.
fn lexed_keywords() -> BTreeSet<String> {
    vilan_core::lexing::KEYWORDS
        .iter()
        .map(|(word, _)| word.to_string())
        .collect()
}

/// Held both ways: `listed` is `lexed_keywords()`, or the assertion says which
/// direction failed and where to fix it.
fn assert_reserved_words_match(listed: &BTreeSet<String>, where_: &str, fix: &str) {
    let lexed = lexed_keywords();
    let missing: Vec<&String> = lexed.difference(listed).collect();
    assert!(
        missing.is_empty(),
        "{where_} is missing {missing:?}. A word a reader cannot see is a word \
         they cannot avoid — {fix}"
    );
    let unknown: Vec<&String> = listed.difference(&lexed).collect();
    assert!(
        unknown.is_empty(),
        "{where_} lists {unknown:?} as reserved, and the lexer does not. A word \
         that stopped being a keyword is a word the docs are telling readers not \
         to use for nothing."
    );
}

/// The SPEC's two reserved-word lists are the lexer's too (tracker N87).
///
/// Only the tour's was gated, and the spec's pair drifted exactly as a
/// hand-copied list does: §A.2 had been missing `css` from the day the css
/// block shipped until a lane added it and `lazy` by hand, and at the moment
/// this gate was written §2.2 was still missing `lazy` — the normative list, in
/// the document that defines what an identifier may be, naming 33 of 34
/// keywords. Nothing could have said so. The tour's gate carried a note
/// arguing the spec's two were outside it because they are "prose-formatted
/// with the boolean and null literals called out separately"; they are not
/// prose, they are `text` fences with the literals inside them and the
/// call-out in a sentence underneath, so there was nothing to except.
///
/// Both lists are held, not one: they are two lists, and a rule that holds the
/// appendix while the normative section drifts is the same failure one level
/// down.
#[test]
fn the_specs_reserved_words_are_the_lexers_keywords() {
    // §2.2 opens with the IDENT grammar's own fence, so the anchor is the
    // sentence that introduces the list rather than the heading above both.
    assert_reserved_words_match(
        &reserved_word_fence("spec/lexical.md", "keyword tokens and are never `IDENT`:"),
        "the spec's §2.2 reserved-word list",
        "add it to the fence in `vilan/docs/spec/lexical.md`, alphabetically, \
         with the two boolean literals last.",
    );
    assert_reserved_words_match(
        &reserved_word_fence("spec/appendix.md", "## A.2 Reserved words"),
        "the spec's §A.2 reserved-word list",
        "add it to the fence in `vilan/docs/spec/appendix.md`, alphabetically, \
         with the two boolean literals last.",
    );
}

/// Unit pins for the fence extractor itself (the indented-fence hardening, D3).
/// These exercise `extract_examples_from` on hand-built markdown so the fence
/// rules are proven directly, independent of what the real docs happen to hold.
/// The mirror in `parse_differential.rs` carries its own copy of these.
#[cfg(test)]
mod extract_pins {
    use super::*;

    /// Build a markdown document from explicit lines (each verbatim, so leading
    /// spaces are exactly as written) with a trailing newline, then extract.
    fn examples(lines: &[&str]) -> Vec<Example> {
        let mut text = lines.join("\n");
        text.push('\n');
        extract_examples_from(&text, Path::new("test.md"))
    }

    #[test]
    fn flush_left_fence_is_unchanged() {
        // The pre-existing behavior: a column-0 fence, body verbatim, heading
        // tracked. Every current doc example is this shape, so it must not move.
        let got = examples(&[
            "# Signals",
            "",
            "```vilan",
            "let x = 1;",
            "```",
            "",
            "after",
        ]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "let x = 1;\n");
        assert_eq!(got[0].heading, "Signals");
        assert_eq!(got[0].line, 3);
    }

    #[test]
    fn bullet_indented_fence_extracts_and_dedents() {
        // A fence indented two columns under a bullet: it is found, and its body
        // is dedented by the fence's own indent so it compiles as flush source.
        let got = examples(&[
            "- A bullet:",
            "",
            "  ```vilan",
            "  let x = 1;",
            "  ```",
            "",
            "- Next bullet",
        ]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "let x = 1;\n");
    }

    #[test]
    fn nested_deeper_body_lines_keep_relative_indent() {
        // Only the fence's columns come off; a line indented deeper than the
        // fence keeps the extra (the body's own structure survives). This is the
        // real shape: a 2-space bullet indent outside, deeper indent inside.
        let got = examples(&[
            "  ```vilan",
            "  fun main() {",
            "      let x = 1;",
            "  }",
            "  ```",
        ]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "fun main() {\n    let x = 1;\n}\n");
    }

    #[test]
    fn an_indented_fence_does_not_swallow_following_prose() {
        // The D3 bug itself: the old close test required a column-0 ```, so an
        // indented fence ran past its real close into the prose and the next
        // fence. Both examples must come back, the prose must stay prose.
        let got = examples(&[
            "- Bullet:",
            "",
            "  ```vilan",
            "  fun first() {}",
            "  ```",
            "",
            "This prose must stay prose.",
            "",
            "```vilan",
            "fun second() {}",
            "```",
        ]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].source, "fun first() {}\n");
        assert_eq!(got[1].source, "fun second() {}\n");
        assert!(got.iter().all(|example| !example.source.contains("prose")));
    }

    #[test]
    fn a_fence_like_line_inside_the_body_at_a_different_indent_does_not_close() {
        // A ``` deeper than the opener is body, not the closer — even though its
        // trimmed text is exactly "```". Only the matching-indent ``` closes, so
        // the block stays open across the impostor.
        let got = examples(&["  ```vilan", "  outer", "    ```", "  more outer", "  ```"]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "outer\n  ```\nmore outer\n");
    }

    #[test]
    fn an_indented_fragment_closes_and_does_not_swallow_the_next_example() {
        // A fragment is not emitted, but its indented fence must still close on
        // the matching indent so a real example after it survives — the exact
        // dev-docs shape (an indented signature fragment, then a real example)
        // that first hit this bug.
        let got = examples(&[
            "- A note:",
            "",
            "  ```vilan,fragment",
            "  signal.map(f)   // just a signature",
            "  ```",
            "",
            "```vilan",
            "fun after() {}",
            "```",
        ]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "fun after() {}\n");
    }
}

/// Names the standard library retired, which its own prose may not go on
/// spelling as if they were live (tracker N88).
///
/// A99 retired six `View` methods in favour of the free slot values, and A95 S2
/// deleted the raw child-relation hatch. The DECLARATIONS went in those orders;
/// the sentences about them did not, and std's doc comments went on teaching
/// `bind_each` — sixteen lines across `reactive.vl` and `rpc.vl`, plus one in
/// `math.vl` — in a library where the doc comment IS the documentation. A
/// reader who writes what the comment says gets "`View` has no method
/// `bind_each`", which is the failure the A99 steer exists to soften and which
/// nothing should have been leading them into.
///
/// Whole identifiers only: `fence_child_relation` is a live helper in
/// `style.vl` and is not this list's business. Comment text only, for the same
/// reason a live identifier is not a retired one.
const RETIRED_STD_NAMES: &[&str] = &[
    "bind_each",
    "bind_each_values",
    "bind_each_by",
    "child_relation",
];

/// The comment lines allowed to name one anyway — each a sentence whose
/// SUBJECT is the retirement, which is the one thing that cannot be said
/// without the old name. Keyed on a distinctive run of the line.
///
/// A run is kept SHORT on purpose (E215): std's prose is re-filled by
/// `vilan fmt` now, so a key spanning most of a line stops matching the
/// moment a word moves across the wrap. A code span is an atom the fill
/// never breaks, so a run anchored on one survives a reflow.
const RETIREMENT_NOTES: &[(&str, &str)] = &[
    (
        "browser/ui.vl",
        "one-line sugar over them — `when`, `swap`, `swap_split`, `bind_each`,",
    ),
    (
        "browser/ui.vl",
        "`bind_each_values`, `bind_each_by` — are retired",
    ),
    ("browser/ui.vl", "Named `each` and not `bind_each`"),
    ("browser/ui.vl", "`{bind_each(..)}` read as a setter"),
    (
        "style.vl",
        "all and pushed authors onto the deleted `child_relation` as a",
    ),
];

#[test]
fn no_std_comment_names_a_retired_method() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std/src");
    let mut files = Vec::new();
    collect_vl_files(&root, &mut files);
    assert!(files.len() > 20, "the std walk found {} files", files.len());

    let mut named = Vec::new();
    for path in &files {
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(path).unwrap_or_default();
        for (number, line) in text.lines().enumerate() {
            let Some(comment) = comment_text(line) else {
                continue;
            };
            if RETIREMENT_NOTES
                .iter()
                .any(|(file, run)| *file == relative && line.contains(run))
            {
                continue;
            }
            for retired in RETIRED_STD_NAMES {
                if names_identifier(comment, retired) {
                    named.push(format!(
                        "  {relative}:{}\n      {retired}: {}",
                        number + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        named.is_empty(),
        "{} std comment line(s) name something std no longer declares. The doc \
         comment IS the documentation here, so a reader who writes what it says \
         gets a refusal. Write the live spelling (`each`, `each_values`, \
         `each_by`, `attribute(..)`), or — if the sentence is ABOUT the \
         retirement — record it in RETIREMENT_NOTES:\n{}",
        named.len(),
        named.join("\n")
    );

    // The inverse (N42's rule): a note excusing a line that no longer exists
    // is an exemption bought for nothing, and it is how a list like this goes
    // quietly out of date.
    let stale: Vec<String> = RETIREMENT_NOTES
        .iter()
        .filter(|(file, run)| {
            !files.iter().any(|path| {
                path.to_string_lossy().replace('\\', "/").ends_with(file)
                    && std::fs::read_to_string(path)
                        .unwrap_or_default()
                        .contains(*run)
            })
        })
        .map(|(file, run)| format!("  {file}: {run:?}"))
        .collect();
    assert!(
        stale.is_empty(),
        "{} retirement note(s) name a line that is gone:\n{}",
        stale.len(),
        stale.join("\n")
    );
}

/// Every `.vl` file under `directory`, recursively.
fn collect_vl_files(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut sorted: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    sorted.sort();
    for path in sorted {
        if path.is_dir() {
            collect_vl_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("vl") {
            out.push(path);
        }
    }
}

/// The comment half of a `.vl` line, or `None` when the line carries none.
///
/// A `//` inside a string literal is not a comment, and std writes plenty of
/// them (urls, selectors); the scan tracks quotes so `"https://…"` is not read
/// as one.
fn comment_text(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if in_string => index += 1,
            b'"' => in_string = !in_string,
            b'/' if !in_string && bytes.get(index + 1) == Some(&b'/') => {
                return Some(&line[index..]);
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Whether `text` names `identifier` as a WHOLE word — `fence_child_relation`
/// does not name `child_relation`.
fn names_identifier(text: &str, identifier: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(offset) = text[from..].find(identifier) {
        let start = from + offset;
        let end = start + identifier.len();
        let before_ok = start == 0 || !is_identifier_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_identifier_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
