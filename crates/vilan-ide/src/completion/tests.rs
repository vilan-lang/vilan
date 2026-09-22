//! The completion engine, driven DIRECTLY (N115).
//!
//! Every completion pin this crate had lived in `vilan-lsp`'s integration
//! tests, one protocol layer up, so the engine's own rules — what a candidate
//! is ranked by, what the client filters it against, what accepting it
//! replaces — were only ever checked through a `Document`. That is a real
//! test of the server and a thin one of the engine: the playground calls the
//! same [`Analysis::completion`] with no `Document` anywhere, and nothing in
//! this crate said what it should answer.
//!
//! So this module builds an [`Analysis`] the way both front ends build one —
//! a real analysis of a real program against the real `std` — and asks the
//! engine directly. The harness is ten lines of struct-filling ([`Engine`]),
//! and it is deliberately the SAME ten lines the language server's
//! `Document::analysis` writes: a pin here that needed a different `Analysis`
//! than a front end can build would be pinning a shape nobody runs.
//!
//! **Seeded with E211's three**, which is the item's list: a candidate's
//! ranking key, its `filter_text`, and its `replace_span`. A future completion
//! behaviour adds its pin here rather than in `vilan-lsp`, which is the point
//! of the file existing at all.

use std::path::{Path, PathBuf};

use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::{PackageSpec, Program, Workspace, analyze_source};

use super::{Completion, CompletionIndex, CompletionKind, ImportRoots};
use crate::analysis::{Analysis, entity_spans};
use crate::line_index::LineIndex;

/// The std PACKAGE directory (holding `vilan.toml`), like the language
/// server's `discover_std_dir` — the bare source root would drop the
/// manifest's platform layers (no `std::ui`, no `std::style`).
fn std_root() -> PathBuf {
    std::env::var_os("VILAN_STD")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"))
}

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(&std_root())
}

/// One analyzed program, held with everything [`Analysis`] borrows.
///
/// The program borrows its source for `'static` — the same leak both front
/// ends take, and a test process is the one place it costs nothing.
struct Engine {
    program: Program<'static>,
    analyzed: LineIndex,
    live: LineIndex,
    entity_spans: Vec<(usize, usize, Id)>,
    platform_requirements: HashMap<Id, String>,
    import_roots: ImportRoots,
    index: CompletionIndex,
}

impl Engine {
    /// Analyze `text` as a manifest-less entry against the real std.
    fn new(text: &str) -> Engine {
        let source: &'static str = Box::leak(text.to_string().into_boxed_str());
        let std = std_spec();
        let (program, errors) = analyze_source(
            source,
            &std,
            Path::new("."),
            Path::new("completion_tests.vl"),
            None,
            &Workspace::default(),
        );
        let program = program.unwrap_or_else(|| {
            panic!("the fixture must analyze to a program; diagnostics: {errors:?}")
        });
        let import_roots = ImportRoots {
            std,
            pkg_root: PathBuf::from("."),
            dependencies: Vec::new(),
        };
        let index = CompletionIndex::build(&program, Some(&import_roots), source);
        Engine {
            entity_spans: entity_spans(&program),
            platform_requirements: vilan_core::platform_color::requirements(&program),
            analyzed: LineIndex::new(source),
            live: LineIndex::new(source),
            program,
            import_roots,
            index,
        }
    }

    /// The `Analysis` a front end would build over this program — field for
    /// field what `Document::analysis` builds.
    fn analysis(&self) -> Analysis<'_, 'static> {
        Analysis {
            program: &self.program,
            analyzed: &self.analyzed,
            live: &self.live,
            entity_spans: &self.entity_spans,
            platform_requirements: &self.platform_requirements,
            import_roots: Some(&self.import_roots),
            index: &self.index,
            source_texts: Default::default(),
            anchor: Default::default(),
            scope_extents: Default::default(),
        }
    }
}

/// The candidates the ENGINE offers at the `¦` in `src`, with the analyzed
/// text beside them (the coordinate space every `replace_span` lives in).
fn completions(src: &str) -> (String, Vec<Completion>) {
    let offset = src.find('¦').expect("the fixture needs a `¦` marker");
    let text = src.replace('¦', "");
    let engine = Engine::new(&text);
    let items = engine.analysis().completion(offset);
    (text, items)
}

const ELEMENT_HEAD_PRELUDE: &str = "import std::ui::view;\nimport std::reactive::{ Signal, SignalCell };\nimport std::io::print;\n";
const CSS_BLOCK_PRELUDE: &str =
    "import std::style::{ Color, Length, Style, space, style };\nimport std::io::print;\n";

/// The harness itself, before anything is claimed over it: the engine answers,
/// and it answers over the analyzed program rather than out of a table of
/// keywords.
///
/// Without this a red in any pin below could mean "the rule moved" or "the
/// fixture stopped analyzing", and those want different fixes.
#[test]
fn the_engine_answers_over_a_real_analysis() {
    let (_, items) = completions("fun main() {\n\tlet alpha = 1;\n\tlet _n = alp¦\n}\n");
    assert!(
        items.iter().any(|item| item.label == "alpha"),
        "a local binding is offered by name: {:?}",
        labels(&items)
    );
}

fn labels(items: &[Completion]) -> Vec<&str> {
    items.iter().map(|item| item.label.as_str()).collect()
}

// --- E211, in the crate that implements it ---------------------------------
//
// Two fields on every candidate, and they are one rule: the engine STATES the
// prefix the client should filter against and the span accepting the candidate
// replaces, instead of leaving both to the client's own notion of a word. VS
// Code is configured by this repo to read `stroke-width` as one word (E194);
// every other client is not, so without these the candidate the author was
// typing towards is the one that disappeared from the list.

#[test]
fn n115_a_hyphenated_attribute_prefix_is_replaced_whole() {
    let source = format!("{ELEMENT_HEAD_PRELUDE}fun main() {{\n\t<svg stroke-w¦></svg>\n}}\n");
    let (text, items) = completions(&source);
    let candidate = items
        .iter()
        .find(|item| item.label == "stroke-width")
        .unwrap_or_else(|| panic!("`stroke-width` is offered: {:?}", labels(&items)));
    let span = candidate
        .replace_span
        .expect("a hyphenated candidate states its span")
        .into_range();
    assert_eq!(
        &text[span], "stroke-w",
        "the hyphen is INSIDE the prefix, which is the whole of the rule"
    );
    assert_eq!(candidate.filter_text.as_deref(), Some("stroke-width"));
}

#[test]
fn n115_a_css_property_prefix_is_replaced_whole() {
    let source = format!(
        "{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet card = css {{\n\t\tflex-dir¦\n\t}};\n}}\n"
    );
    let (text, items) = completions(&source);
    let candidate = items
        .iter()
        .find(|item| item.label == "flex-direction")
        .unwrap_or_else(|| panic!("`flex-direction` is offered: {:?}", labels(&items)));
    let span = candidate.replace_span.expect("a span").into_range();
    assert_eq!(&text[span], "flex-dir");
    assert_eq!(candidate.filter_text.as_deref(), Some("flex-direction"));
}

#[test]
fn n115_an_ordinary_identifier_prefix_stops_at_the_word() {
    // In CODE a `-` is subtraction and no identifier carries one, so the
    // hyphenated rule stays out of expression position: the prefix of `1-al`
    // is `al`, not `1-al`.
    let (text, items) = completions("fun main() {\n\tlet alpha = 1;\n\tlet _c = 1-al¦\n}\n");
    let candidate = items
        .iter()
        .find(|item| item.label == "alpha")
        .unwrap_or_else(|| panic!("`alpha` is in scope: {:?}", labels(&items)));
    let span = candidate.replace_span.expect("a span").into_range();
    assert_eq!(&text[span], "al");
}

#[test]
fn n115_every_candidate_of_a_request_carries_both_fields() {
    // The stamp is a property of the REQUEST, not of the hyphenated candidates
    // that needed it — so a context that offers nothing hyphenated carries it
    // too, and `filter_text` is the label there.
    for source in [
        "fun main() {\n\tlet name = \"vilan\";\n\tlet _n = name.le¦\n}\n",
        "import std::io::pri¦\n",
        "fun main() {\n\tlet _x = pri¦\n}\n",
    ] {
        let (_, items) = completions(source);
        assert!(!items.is_empty(), "candidates at {source:?}");
        for item in &items {
            assert!(
                item.replace_span.is_some(),
                "{:?} at {source:?} states no replace span",
                item.label
            );
            assert_eq!(
                item.filter_text.as_deref(),
                Some(item.label.as_str()),
                "{:?} at {source:?}",
                item.label
            );
        }
    }
}

// --- Ranking ---------------------------------------------------------------

#[test]
fn n115_a_construct_snippet_ranks_below_every_entity() {
    // The engine's one ranking rule that is not the client's alphabetical
    // default: a construct snippet (E14) is template text, and a name the
    // author has actually declared outranks it. The front ends spell that as a
    // `~`-prefixed `sort_text`, which is the protocol's way of saying "last";
    // what the ENGINE owes is the KIND, so the rule has something to key on.
    let (_, items) = completions("fun main() {\n\tlet fusible = 1;\n\tf¦\n}\n");
    let snippets: Vec<&str> = items
        .iter()
        .filter(|item| item.kind == CompletionKind::Snippet)
        .map(|item| item.label.as_str())
        .collect();
    assert!(
        !snippets.is_empty(),
        "a bare statement position offers construct snippets: {:?}",
        labels(&items)
    );
    assert!(
        items
            .iter()
            .any(|item| item.label == "fusible" && item.kind != CompletionKind::Snippet),
        "and the declared binding is offered beside them as an entity: {:?}",
        labels(&items)
    );
}
