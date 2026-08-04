//! The handwritten frontend's corpus-scale regression sweep + fmt tripwire
//! (`proposal/frontend.md` §3 S5).
//!
//! Through the H6 arc this was a differential against the chumsky ORACLE, proving
//! the handwritten frontend byte-identical over every real source. At the S5
//! cutover chumsky is deleted, so the oracle arm retires: this target becomes the
//! new parser's own regression corpus — every `*.vl` in the repo, every std layer,
//! every example, every compilable docs fence, and the corpus-absent construct set
//! must parse CLEAN (a tree, zero diagnostics) through `parsing::parse`, with no
//! panic. The trees themselves were proven byte-identical to chumsky's at S3 and
//! are re-checked end-to-end by the corpus byte-gate (`vilan-cli --test corpus`);
//! this sweep guards the *front* of the pipeline — that the whole clean corpus
//! still parses without error or panic.
//!
//! The fmt tripwire converts the formatter's silent-no-op failure mode (§0: the
//! re-lex-and-compare safety net turns `fmt` into a no-op when the token stream
//! drifts, indistinguishable from an already-canonical file) into loud, external
//! checks: `formatter_output_token_matches_input` guards against token-drifting
//! output, and `formatter_never_silently_bails` (the E13 closing gate, live
//! since 2026-07-22) asserts `fmt` never silently no-ops.
//!
//! Both watch [`formattable_files`] — the corpus, std, the examples and the
//! `vilan init` templates. They watched the CORPUS ALONE until 2026-08-01, and
//! that gap is how five std files sat in a silent bail (backlog 47): the corpus
//! is where regressions are deliberately planted, not where the language's own
//! source lives.

use std::path::{Path, PathBuf};
use vilan_core::token::Token;
use vilan_core::{formatter, lexing, parsing};

// ---------------------------------------------------------------------------
// Source enumeration: the corpus, every std layer, examples, and docs examples
// ---------------------------------------------------------------------------

fn repo_vilan() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan")
}

/// Every `*.vl` under `root`, recursively, sorted for a stable summary.
fn collect_vl(root: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_vl(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "vl") {
            into.push(path);
        }
    }
}

/// The count of leading ASCII spaces on `line` — the fence-indent measure. Tabs
/// are not counted (the book indents fences with spaces). Mirror of
/// `docs.rs::leading_spaces`; see the NOTE on `collect_doc_examples`.
fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

/// Strip up to `indent` leading spaces from `line` (CommonMark's fenced-code
/// dedent, §4.5). Mirror of `docs.rs::dedent`.
fn dedent(line: &str, indent: usize) -> &str {
    &line[leading_spaces(line).min(indent)..]
}

/// A fenced block in progress while scanning one markdown document.
struct DocFence {
    compile: bool,
    opened_at: usize,
    indent: usize,
    body: String,
}

/// Extract the compilable fenced examples from one markdown document's `text`,
/// pushing `(label, dedented_body)` for each. This is the same fence logic as
/// `docs.rs::extract_examples_from` — an indent-tracked open, a SAME-indent
/// close, and CommonMark dedent — reduced to the compile-eligible set the parse
/// sweep needs. Kept as a small pure function so it can be unit-pinned.
fn doc_examples_from(text: &str, file_label: &str, into: &mut Vec<(String, String)>) {
    let mut fence: Option<DocFence> = None;
    for (index, line) in text.lines().enumerate() {
        match &mut fence {
            Some(open) => {
                // Close only on a fence at the opener's own indent; the indent
                // check is first so the slice below stays in bounds.
                if leading_spaces(line) == open.indent && line[open.indent..].trim_end() == "```" {
                    if open.compile {
                        let label = format!("docs:{file_label}:{}", open.opened_at + 1);
                        into.push((label, std::mem::take(&mut open.body)));
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
                    let compile = matches!(info.trim(), "vilan" | "vilan,norun" | "vilan,browser");
                    fence = Some(DocFence {
                        compile,
                        opened_at: index,
                        indent,
                        body: String::new(),
                    });
                }
            }
        }
    }
    // An unclosed fence is the docs gate's error to report (docs.rs asserts on
    // it); here a stray tail simply yields no further example.
}

/// Every compilable fenced example under `vilan/docs/**` (plus the repo README),
/// as `(label, source)` — `vilan` / `vilan,norun` / `vilan,browser` are complete
/// programs (compiled by the docs gate, hence clean to parse); `vilan,fragment`
/// and non-vilan fences are skipped.
///
/// NOTE — keep in sync with `docs.rs`'s `extract_examples_from`: the fence rules
/// (indent-tracked open, same-indent close, CommonMark dedent) are duplicated
/// here because `docs.rs` is a separate test target, not a library. The two MUST
/// extract the same example set; a change to the fence logic belongs in both.
fn collect_doc_examples(into: &mut Vec<(String, String)>) {
    let docs_root = repo_vilan().join("docs");
    let mut markdown = Vec::new();
    collect_markdown(&docs_root, &mut markdown);
    let readme = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    if readme.is_file() {
        markdown.push(readme);
    }
    for file in &markdown {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        doc_examples_from(&text, &file.display().to_string(), into);
    }
}

fn collect_markdown(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "book") {
                continue; // rendered-site output, not content
            }
            collect_markdown(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            into.push(path);
        }
    }
}

/// Whole-file S3 constructs that the repo corpus happens NOT to exercise (so the
/// file-derived sweep never reaches them), each a clean program the parser must
/// accept. Only PARSED here (types need not resolve), so bare type names are fine.
/// This closes the corpus's coverage gaps — notably `[trait_only]` / `[doc(hidden)]`
/// (zero corpus uses) and the tuple-bound endpoint variants — alongside the
/// (durable) in-module pins in `parsing.rs`.
fn corpus_absent_constructs() -> Vec<(String, String)> {
    [
        // The two attributes with zero corpus uses.
        ("trait_only", "trait Surface { [trait_only] fun hidden(&self): i32; }"),
        ("doc_hidden", "[doc(hidden)] fun helper(): i32 { 0 }"),
        // Every function attribute at once, in the one legal (fixed) order.
        (
            "all_attributes",
            "[extern(\"m\", \"s\")] [must_use] [rpc] [trait_only] [doc(hidden)] [platform(\"@process\", \"browser\")] external fun everything(): i32;",
        ),
        // Tuple-bound endpoint variants: both, hi-only, and an element bound.
        ("tuple_bound_both", "fun a<T: (2..10)>(): T { default() }"),
        ("tuple_bound_hi", "fun b<T: (..10)>(): T { default() }"),
        ("tuple_bound_element", "fun c<T: (..: Show)>(): T { default() }"),
        // Spread parameters (variadic-generics.md §S): the marker is three `.`
        // control tokens with no node of its own, in every legal shape — bare,
        // after a fixed parameter with `mut`, and over a mapped pack.
        ("spread_bare", "fun d<T: (..: Show)>(...items: T): i32 { 0 }"),
        ("spread_after_fixed", "fun e<T: (2..)>(sep: str, mut ...rest: T): i32 { 0 }"),
        (
            "spread_mapped",
            "fun f<T: (2..)>(...sources: (U in T: Signal<U>)): Signal<T> { gather(sources) }",
        ),
        // Tuple-value spreads (variadic-generics.md §T): two `.` control tokens
        // where an element begins, in every position the rule admits — leading,
        // trailing, interleaved, twice, alone, and at a call site.
        ("spread_value_lead", "fun g(): i32 { let t = (..a, b); 0 }"),
        ("spread_value_trail", "fun h(): i32 { let t = (b, ..a); 0 }"),
        ("spread_value_mid", "fun i(): i32 { let t = (b, ..a, c); 0 }"),
        ("spread_value_twice", "fun j(): i32 { let t = (..a, ..b); 0 }"),
        ("spread_value_lone", "fun k(): i32 { let t = (..a); 0 }"),
        ("spread_value_call", "fun l(): i32 { pack(..pair, 7) }"),
        // A generic default and a `type` binder default together.
        ("generic_defaults", "struct Cell<T = Self, type U = i32> { value: T }"),
        // Import/use path shapes: a top-level set, a deeply nested set, a use set.
        ("import_top_set", "import { alpha, beta };"),
        ("import_nested_set", "import root::mid::{ leaf, twig::{ a, b } };"),
        ("use_set", "use collection::{ Map, Set };"),
        // Every parameter convention in one signature (own + & + &mut + inferred).
        (
            "conventions",
            "fun mix(own a: A, &b: B, &mut c: C, d: D, e: &E): i32 { 0 }",
        ),
        // The `null`-named bodyless external struct and the full resource modifier.
        ("external_null", "external struct null;"),
        ("resource_external", "resource external struct Handle;"),
        ("resource_enum", "resource enum State { Open, Closed }"),
        // An enum with negative + explicit discriminants alongside a payload.
        (
            "enum_discriminants",
            "enum Ordering { Less = -1, Equal = 0, Greater(i32) }",
        ),
        // A tuple comprehension as a value, and macro forms in both positions.
        ("tuple_comprehension", "fun t(): T { (x in xs => x + 1) }"),
        (
            "macro_forms",
            "macro fun make(): Source { source(\"\") }\nmacro grow(a, b)\nfun use_it() { let v = macro pick(x); macro { ret void } }",
        ),
        // `export` wrapping several item kinds, and a nested module.
        ("export_items", "export struct S { x: i32 }\nexport fun f() { }\nexport use m::n;"),
        (
            "nested_module",
            "mod outer { mod inner { fun deep() { } } struct Local { n: i32 } }",
        ),
    ]
    .into_iter()
    .map(|(label, source)| (format!("adversarial:{label}"), source.to_string()))
    .collect()
}

/// The full sweep corpus: `(label, source)` over `vilan/test`, every
/// `vilan/std/src` layer, `vilan/examples`, the docs examples, and the
/// corpus-absent S3 constructs above.
fn all_sources() -> Vec<(String, String)> {
    let vilan = repo_vilan();
    let mut files = Vec::new();
    collect_vl(&vilan.join("test"), &mut files);
    collect_vl(&vilan.join("std/src"), &mut files);
    collect_vl(&vilan.join("examples"), &mut files);
    let mut sources: Vec<(String, String)> = files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            Some((path.display().to_string(), text))
        })
        .collect();
    collect_doc_examples(&mut sources);
    sources.extend(corpus_absent_constructs());
    sources
}

// ---------------------------------------------------------------------------
// The regression sweep
// ---------------------------------------------------------------------------

#[test]
fn the_handwritten_frontend_parses_every_clean_source() {
    let sources = all_sources();
    assert!(
        sources.len() > 150,
        "suspiciously few sources enumerated: {}",
        sources.len()
    );

    // Every enumerated source is a complete, valid program (it compiles, or is a
    // parse-only adversarial construct), so it must parse CLEAN: a tree comes back
    // (always — the frontend never discards), with an EMPTY diagnostic list. A
    // non-empty list means the parser rejects a source it must accept — a real
    // regression, localized by label. `parsing::parse` never panics on any input
    // (the recovery contract), so reaching the end at all is part of the sweep.
    let mut rejected: Vec<String> = Vec::new();
    let mut clean = 0usize;
    for (label, source) in &sources {
        let (tree, errors) = parsing::parse(source);
        if tree.is_none() {
            rejected.push(format!("{label}: no tree returned"));
        } else if !errors.is_empty() {
            rejected.push(format!(
                "{label}: {} diagnostic(s) on a clean source: {}",
                errors.len(),
                parsing::render(&errors[0])
            ));
        } else {
            clean += 1;
        }
    }

    eprintln!(
        "parse sweep (handwritten frontend): N={} sources, {} parsed clean",
        sources.len(),
        clean
    );
    assert!(
        rejected.is_empty(),
        "the handwritten frontend rejected {} source(s) it must accept:\n{}",
        rejected.len(),
        rejected.join("\n")
    );
}

// ---------------------------------------------------------------------------
// The fmt tripwire
// ---------------------------------------------------------------------------

/// The formatter's own notion of "the same code": the lexer's token stream with
/// spans stripped. Re-implemented here against the PUBLIC `lexing::tokenize` so the
/// check is external to `formatter.rs` (the point of a tripwire). Mirrors
/// `formatter::code_tokens` + `formatter::normalize`.
fn normalized_tokens(source: &str) -> Option<Vec<Token<'_>>> {
    let (spanned, lex_errors) = lexing::tokenize(source);
    if !lex_errors.is_empty() {
        return None;
    }
    let tokens: Vec<Token<'_>> = spanned.into_iter().map(|(token, _span)| token).collect();
    // A trailing comma before a closer is insignificant in vilan — the formatter
    // may normalize it in or out, so the safety check ignores it.
    let mut result: Vec<Token<'_>> = Vec::with_capacity(tokens.len());
    for token in tokens {
        if matches!(
            token,
            Token::Ctrl('}') | Token::Ctrl(')') | Token::Ctrl(']')
        ) {
            while let Some(Token::Ctrl(',')) = result.last() {
                result.pop();
            }
        }
        result.push(token);
    }
    Some(result)
}

fn corpus_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_vl(&repo_vilan().join("test"), &mut files);
    files
}

/// Every `*.vl` the formatter is expected to handle faithfully: the regression
/// corpus, the standard library, the examples, and the embedded `vilan init`
/// templates.
///
/// The two tripwires below used to watch the CORPUS ALONE, and that is exactly
/// how five std files sat in a silent bail long enough to be discovered by
/// accident (backlog 47) — `browser/ui.vl`, `option.vl`, `process/ui.vl`,
/// `reactive.vl`, `task.vl`, between them a `context` clause, a mapped type, a
/// tuple comprehension, a tuple-arity bound, and a written `void` the printer
/// dropped. Two of them are `formatter::idempotency` fixtures, whose fixed-point
/// assertion a bailing file satisfies trivially, so the pins agreed with the
/// silence. The corpus is where regressions are DESIGNED to land; it is not
/// where the language's own source lives.
fn formattable_files() -> Vec<PathBuf> {
    let mut files = corpus_files();
    collect_vl(&repo_vilan().join("std"), &mut files);
    collect_vl(&repo_vilan().join("examples"), &mut files);
    collect_vl(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vilan-cli/templates"),
        &mut files,
    );
    files
}

/// A repo-relative label for a file in [`formattable_files`]. Base names are not
/// unique across the roots (`std/src/browser/ui.vl` and `std/src/process/ui.vl`
/// are both `ui.vl`), so a failure has to say which one.
fn label(path: &Path) -> String {
    let full = path.to_string_lossy().replace('\\', "/");
    match full.rfind("/vilan/") {
        Some(at) => full[at + 1..].to_string(),
        None => match full.rfind("/crates/") {
            Some(at) => full[at + 1..].to_string(),
            None => full,
        },
    }
}

#[test]
fn formatter_output_token_matches_input() {
    // The durable tripwire: whatever `format` returns for a corpus file, its token
    // stream must match the input's (unchanged output matches trivially; a
    // successful reprint matches by the formatter's contract). This catches any
    // token-drifting output that slips the formatter's internal safety net.
    //
    // The formatter canonicalizes top-level import-run order, so a reprint's
    // tokens may legitimately differ from the input's by that reorder alone. We
    // check order-sensitively FIRST (the strong, independent test that most files
    // pass trivially); only when the raw streams differ do we fall back to the
    // net's own import-run canonicalization (`formatter::sort_import_runs`, the
    // shared implementation) to confirm the difference is import order and
    // nothing else. That fallback leaves non-import tokens in place, so a genuine
    // non-import reordering still diverges and still fires this tripwire.
    let files = formattable_files();
    assert!(
        files.len() > 150,
        "suspiciously few formattable files: {}",
        files.len()
    );
    let mut mismatches: Vec<String> = Vec::new();
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let output = formatter::format(&source);
        if output == source {
            continue; // unchanged: trivially token-equal
        }
        let (Some(input_tokens), Some(output_tokens)) =
            (normalized_tokens(&source), normalized_tokens(&output))
        else {
            mismatches.push(format!("{} (did not lex)", label(path)));
            continue;
        };
        if input_tokens == output_tokens {
            continue; // token-equal modulo trailing commas — no import reorder
        }
        // The streams differ: the only legitimate cause is import-run reordering.
        if formatter::sort_import_runs(&input_tokens) != formatter::sort_import_runs(&output_tokens)
        {
            mismatches.push(label(path));
        }
    }
    assert!(
        mismatches.is_empty(),
        "formatter output token-DRIFTED from the input on {} file(s): {:?}",
        mismatches.len(),
        mismatches
    );
}

/// The corpus files the formatter currently BAILS on, by base name (sorted).
///
/// Detector: `format` is a total canonicalizer over parseable input, so it must
/// map a source and a token-preserving perturbation of it to the SAME output.
/// Appending blank lines is such a perturbation (trailing newlines are trivia,
/// always normalized away, and change no comment). If `format(source)` and
/// `format(source + "\n\n")` DIFFER, the formatter bailed on this file — it
/// returned each input verbatim (with the extra newlines surviving) instead of
/// canonicalizing. A truly-canonical file is NOT flagged: both map to itself.
/// (Verified: every flagged file returns BOTH inputs verbatim — `format(x)==x` —
/// while controls strip the perturbation, the clean bail-vs-canonical signal.)
fn current_bail_set() -> Vec<String> {
    let mut bails: Vec<String> = formattable_files()
        .into_iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).ok()?;
            let base = formatter::format(&source);
            let perturbed = formatter::format(&format!("{source}\n\n"));
            (base != perturbed).then(|| label(&path))
        })
        .collect();
    bails.sort();
    bails
}

/// The corpus files `vilan fmt` silently no-ops on — the ledger is EMPTY and the
/// assertion below keeps it that way.
///
/// E13 closed nine of the ten H6 S0 bailers on 2026-07-22 by adding the missing
/// printer arms (destructuring, fixed arrays, macro forms, unary minus, and the
/// lift-chain postfix subject). The tenth, `numeric-types.vl`, was a DESIGN gap
/// rather than a missing arm: a redundant paren group in a value position —
/// `(300).as_u8()`, `let b = (1 + 2);` — was dissolved by the parser and so
/// unrecorded in the AST, the printer put back only the parens precedence
/// demanded, and the net (which compares the OUTPUT's tokens to the SOURCE's)
/// refused the reprint and returned the whole file's original bytes. E13
/// canonicalized the corpus's four such sites away (emission byte-identical,
/// probe-proven) and recorded the gap.
///
/// That gap is now CLOSED at the root the note predicted: the formatter parses
/// in group-preserving mode (`parsing::parse_preserving_groups`), which records
/// every `(…)` as a node, so a user-written group reprints as written. The
/// corpus carries no such shape today (it was canonicalized), so this gate does
/// not exercise the fix — `formatter::paren_groups` pins it per shape.
#[test]
fn formatter_never_silently_bails() {
    let bails = current_bail_set();
    assert!(
        bails.is_empty(),
        "formatter SILENTLY BAILED on {} corpus file(s): {:?}",
        bails.len(),
        bails
    );
}

// ---------------------------------------------------------------------------
// Doc-fence extractor pins (D3) — this file's copy of the fence logic. Kept in
// step with `docs.rs::extract_pins`; the two extractors must agree.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod doc_fence_pins {
    use super::*;

    fn doc_examples(lines: &[&str]) -> Vec<(String, String)> {
        let mut text = lines.join("\n");
        text.push('\n');
        let mut out = Vec::new();
        doc_examples_from(&text, "test.md", &mut out);
        out
    }

    #[test]
    fn pd_flush_fence_is_unchanged() {
        let got = doc_examples(&["```vilan", "let x = 1;", "```"]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "let x = 1;\n");
    }

    #[test]
    fn pd_bullet_indented_fence_dedents_keeping_relative_indent() {
        let got = doc_examples(&[
            "  ```vilan",
            "  fun main() {",
            "      let x = 1;",
            "  }",
            "  ```",
        ]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "fun main() {\n    let x = 1;\n}\n");
    }

    #[test]
    fn pd_indented_fence_does_not_swallow_the_following_example() {
        let got = doc_examples(&[
            "- note:",
            "",
            "  ```vilan",
            "  fun first() {}",
            "  ```",
            "",
            "Prose in between.",
            "",
            "```vilan",
            "fun second() {}",
            "```",
        ]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1, "fun first() {}\n");
        assert_eq!(got[1].1, "fun second() {}\n");
    }

    #[test]
    fn pd_indented_fragment_is_skipped_but_still_closes() {
        // A fragment is not a sweep example, but its indented fence must still
        // close so a following real example survives (docs.rs's mirror case).
        let got = doc_examples(&[
            "- note:",
            "",
            "  ```vilan,fragment",
            "  signal.map(f)",
            "  ```",
            "",
            "```vilan",
            "fun after() {}",
            "```",
        ]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "fun after() {}\n");
    }
}
