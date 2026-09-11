//! Semantic preservation for the element-head sort (E151), proved rather than
//! argued: for elements written OUT of the canonical order, building and
//! RUNNING the source and building and running its formatted twin must produce
//! the same document.
//!
//! The sibling of `style_chain_order.rs`, and the same two assertions, with the
//! same division of labour:
//!
//!   1. **The rendered document is the same.** Two builds' stdout, compared
//!      once the attributes inside each tag are sorted — which is exactly what
//!      the reorder is ALLOWED to change, because an HTML start tag's
//!      attributes are a set and their written order carries no meaning. This
//!      is the strong one: every attribute, every handler slot, every child and
//!      every text node has to survive with its value, and a LAST-WINS pair
//!      that crossed shows up as a different value rather than a different
//!      order. `ORDER_SENSITIVE` is written so that a wrong BARRIER rule flips
//!      `class="from-attribute"` to `class="from-link"` and this assertion says
//!      so.
//!
//!   2. **Nothing was added or dropped.** (1) compares what the program PRINTS,
//!      and a SERVER-rendered element carries no handler wiring at all
//!      (`std::ui`'s `on` accepts and discards), so an `on:` handler is
//!      invisible to it. The emitted JavaScript is therefore compared as two
//!      multisets — every string literal it carries, and every character — so
//!      an item that was dropped, duplicated, or had its name or value altered
//!      is red here even where the rendering could not see it. What it
//!      deliberately does not catch is a PERMUTATION, which is (1)'s job for
//!      everything that renders and the in-crate pins' job for the rest.
//!
//! One consequence of the reorder is worth naming rather than discovering: an
//! attribute's VALUE is an arbitrary expression, and moving the item moves when
//! that expression is evaluated. Attribute values in practice are literals and
//! signal reads, and the same is true of the style chain's arguments, so this
//! is the same bargain that order shipped with — but a value with a side effect
//! whose ORDER matters is outside what either sorter promises.
//!
//! E156 asked whether to accept that or to refuse to sort a head whose values
//! are not literals or pure paths, and it is RULED (2026-09-11) accept and
//! document. [`sorting_an_element_head_moves_when_its_values_are_evaluated`] is
//! the bargain as a FACT rather than as this paragraph: it prints from two
//! attribute values and asserts that the sorted twin ran them in the SORTED
//! order while rendering the same document. A purity check added later without
//! a ruling reds there, which is the point of pinning a bargain.
//!
//! The order itself is pinned in `vilan-core`'s `formatter::element_head_layout`.

use std::path::{Path, PathBuf};
use std::process::Command;

use vilan_core::formatter::{element_head_permutation, element_heads};

/// The tracked sources that carry element markup. Each is built and run as
/// tracked and again through the formatter, and the two must render the same
/// document.
///
/// These are IN canonical order (the tree was reformatted when the order
/// shipped), so the formatter is a no-op on their heads and the comparison is a
/// regression guard rather than a demonstration: it re-acquires its teeth the
/// moment a head here is written out of order or the canonical order changes.
/// [`every_tracked_fixture_is_already_in_canonical_order`] is what holds them
/// canonical, and the demonstration lives in [`ORDER_SENSITIVE`] — a fixture
/// written out of order on purpose, in every shape where a wrong rule would
/// change what renders.
const ELEMENT_SOURCES: &[&str] = &["vilan/test/element-syntax.vl"];

/// The demonstration the tracked fixture can no longer be, now that it is
/// canonical: elements written OUT of order on purpose, built and run both
/// ways.
///
/// Every line is a shape where a wrong rule would change the document:
///
///   * the BARRIER, in both directions — an undotted `class` on either side of
///     a `.class(…)` link. The link and the attribute write the same slot, the
///     formatter knows nothing about what a dotted link writes, and the one
///     written LAST wins. Let either cross the other and the rendered class
///     flips;
///   * two dotted links whose order IS the document (`.child` appends), with
///     sortable attributes on both sides of them, so the runs sort while the
///     links hold their positions;
///   * a LAST-WINS pair — the same attribute name twice — which a STABLE sort
///     must keep in written order;
///   * every leading name in [the canonical order] written backwards, mixed
///     with a keyword-spelled name (`for`, `type`) and hyphenated ones
///     (`data-x`, `aria-y`), which are several tokens each;
///   * `on:` handlers written BEFORE the attributes and out of alphabetical
///     order among themselves;
///   * a bare boolean attribute, whose value is empty and which still holds a
///     slot.
const ORDER_SENSITIVE: &str = concat!(
    "import std::io::print;\n",
    "import std::ui::{ View, render, view };\n",
    "\n",
    "fun main() {\n",
    // The barrier, both ways round. As written the attribute wins the first and
    // the link wins the second; a crossing swaps them.
    "\tprint(render(<div .class(\"from-link\") class(\"from-attribute\")>\"a\"</div>));\n",
    "\tprint(render(<div class(\"from-attribute\") .class(\"from-link\")>\"b\"</div>));\n",
    // Two links whose order is the document, with a run on either side.
    "\tprint(render(<ul title(\"t\") .child(<li>\"one\"</li>) .child(<li>\"two\"</li>) ",
    "id(\"i\")>\"c\"</ul>));\n",
    // The leading names, a keyword name, a bare boolean attribute.
    "\tprint(render(<input type(\"checkbox\") disabled aria-label(\"Done\") name(\"n\") ",
    "id(\"i\") />));\n",
    // Handlers before the attributes, and out of order among themselves.
    "\tprint(render(<a on:click(|| print(\"go\")) href(\"/x\") class(\"nav\") id(\"home\") ",
    "on:blur(|| print(\"bye\"))>\"go\"</a>));\n",
    // Every leading name, backwards, with hyphenated names after them.
    "\tprint(render(<label for(\"f\") data-x(\"1\") aria-y(\"2\") src(\"s\") name(\"n\") ",
    "href(\"h\") type(\"t\") id(\"i\")>\"l\"</label>));\n",
    // The last-wins pair: `second` renders, and only a stable sort keeps it so.
    "\tprint(render(<div class(\"first\") class(\"second\")>\"dup\"</div>));\n",
    "}\n",
    "main();\n",
);

/// E156's fixture: two attribute values that PRINT, written out of canonical
/// order. `title` is written first and `id` leads the canonical order, so the
/// sort moves them past each other and the two `print`s swap.
const EVALUATION_ORDER: &str = concat!(
    "import std::io::print;\n",
    "import std::ui::{ View, render, view };\n",
    "\n",
    "fun note(name: str): str {\n",
    "\tprint(name);\n",
    "\tname\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(render(<div title(note(\"title\")) id(note(\"id\"))>\"x\"</div>));\n",
    "}\n",
    "main();\n",
);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repo root is two levels above the crate")
}

fn std_dir() -> PathBuf {
    repo_root().join("vilan/std")
}

/// A scratch directory nothing else in this binary can be using — the process
/// id alone is not enough, because nextest runs this binary's tests
/// concurrently in ONE process and two of them sweep the same fixture list.
fn scratch_directory(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

/// One build: the emitted JavaScript, and what running it prints.
struct Built {
    javascript: String,
    rendered: String,
}

/// Compiles the bare corpus program at `source` and runs what it emitted.
fn build_and_run(source: &Path) -> Built {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(source)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        output.status.success(),
        "{} did not build:\n{}",
        source.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let emitted = source.with_extension("mjs");
    let javascript = std::fs::read_to_string(&emitted)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", emitted.display()));
    let run = Command::new("node")
        .arg(&emitted)
        .output()
        .expect("run the emitted program with node");
    assert!(
        run.status.success(),
        "{} did not run:\n{}",
        emitted.display(),
        String::from_utf8_lossy(&run.stderr)
    );
    Built {
        javascript,
        rendered: String::from_utf8_lossy(&run.stdout).to_string(),
    }
}

/// One fixture built and run twice — as written, and formatted.
struct Twins {
    written: Built,
    sorted: Built,
}

/// Writes `fixture` and its formatted twin into two scratch directories, then
/// builds and runs each. `expect_reorder` refuses a fixture the formatter
/// leaves alone, so a demonstration that stopped demonstrating is red rather
/// than quietly vacuous.
fn twins(label: &str, fixture: &str, expect_reorder: bool) -> Twins {
    let temporary = scratch_directory(&format!("vilan-element-head-{label}"));
    let written_dir = temporary.join("written");
    let sorted_dir = temporary.join("sorted");
    std::fs::create_dir_all(&written_dir).expect("create the written directory");
    std::fs::create_dir_all(&sorted_dir).expect("create the sorted directory");
    let formatted = vilan_core::formatter::format(fixture);
    assert_eq!(
        formatted != fixture,
        expect_reorder,
        "{label} did not reorder as expected"
    );
    let written_source = written_dir.join("fixture.vl");
    let sorted_source = sorted_dir.join("fixture.vl");
    std::fs::write(&written_source, fixture).expect("write the fixture");
    std::fs::write(&sorted_source, &formatted).expect("write the sorted twin");
    let written = build_and_run(&written_source);
    let sorted = build_and_run(&sorted_source);
    let _ = std::fs::remove_dir_all(&temporary);
    Twins { written, sorted }
}

/// `markup` with the attributes inside every start tag sorted — the one
/// difference a legal reorder may produce, and nothing else. An HTML start tag
/// reads its attributes as a set, so two documents equal under this are the
/// same document.
///
/// Deliberately naive, and adequate for exactly this input: the fixtures print
/// serialized markup with no `>` inside an attribute value and no raw text
/// element, so a tag is `<` up to the first `>`.
fn sort_tag_attributes(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let Some(close) = rest[at..].find('>') else {
            out.push_str(&rest[at..]);
            return out;
        };
        let tag = &rest[at + 1..at + close];
        let mut parts = split_attributes(tag);
        if parts.len() > 1 {
            let head = parts.remove(0);
            parts.sort();
            out.push('<');
            out.push_str(&head);
            for part in parts {
                out.push(' ');
                out.push_str(&part);
            }
            out.push('>');
        } else {
            out.push('<');
            out.push_str(tag);
            out.push('>');
        }
        rest = &rest[at + close + 1..];
    }
    out.push_str(rest);
    out
}

/// A start tag's interior split at the spaces OUTSIDE a quoted value, so a
/// value holding a space travels with its name.
fn split_attributes(tag: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in tag.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            ' ' if !quoted => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// A whole emitted program reduced to two multisets: every string literal it
/// carries, sorted, and every character, sorted. Two programs equal under this
/// hold exactly the same material — nothing added, nothing dropped, no name or
/// value altered — and differ at most by a permutation.
///
/// Deliberately whole-file rather than per line: a handler is a closure with a
/// multi-line body, so moving one legitimately moves LINES, and a per-line
/// comparison would call that a difference.
fn javascript_shape(javascript: &str) -> (Vec<String>, Vec<char>) {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for character in javascript.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
                literals.push(std::mem::take(&mut current));
                continue;
            }
            current.push(character);
            continue;
        }
        if character == '"' {
            in_string = true;
        }
    }
    literals.sort();
    let mut characters: Vec<char> = javascript.chars().collect();
    characters.sort_unstable();
    (literals, characters)
}

/// The headline invariant: a reordered head renders the same document.
#[test]
fn sorting_an_element_head_renders_the_same_document() {
    let twins = twins("sensitive", ORDER_SENSITIVE, true);
    assert_eq!(
        sort_tag_attributes(&twins.written.rendered),
        sort_tag_attributes(&twins.sorted.rendered),
        "the canonical order changed what the order-sensitive fixture renders. Attribute ORDER \
         inside a tag is the one thing a reorder may change; anything else means an item crossed \
         a dotted link, or a last-wins pair crossed itself. This is the fixture the barrier rule \
         and the stable sort exist for.\n--- written ---\n{}\n--- sorted ---\n{}",
        twins.written.rendered,
        twins.sorted.rendered
    );
    // Non-vacuity: the two really did render DIFFERENT bytes, so the assertion
    // above is comparing two orders rather than one string with itself.
    assert_ne!(
        twins.written.rendered, twins.sorted.rendered,
        "the order-sensitive fixture rendered byte-identical output, so the comparison above \
         proves nothing about the canonicalization"
    );
}

/// The structural invariant: the same material, permuted. The same string
/// literals and the same characters — so an item that was dropped, duplicated,
/// or had its name or value altered is red here even where the rendering above
/// could not see it, which is the only way an `on:` handler is covered at all
/// (a server-rendered element discards its handlers).
#[test]
fn sorting_an_element_head_adds_and_drops_nothing() {
    let twins = twins("sensitive", ORDER_SENSITIVE, true);
    assert_eq!(
        javascript_shape(&twins.written.javascript),
        javascript_shape(&twins.sorted.javascript),
        "the canonical order changed the emitted JavaScript by more than a permutation — an \
         item was dropped, duplicated, or had its name or value altered."
    );
    assert_ne!(
        twins.written.javascript, twins.sorted.javascript,
        "the order-sensitive fixture emitted byte-identical JavaScript, so the comparison above \
         proves nothing"
    );
}

/// The tracked corpus, both ways. Canonical today, so the twins are identical
/// and this is a regression guard; it goes red the moment a head in one of
/// these files is written out of order.
/// E156, ruled 2026-09-11: the sorter reorders WHEN attribute values run, and
/// that is accepted and documented rather than checked for.
///
/// Pinned here because a documented bargain nobody checks is a sentence, not a
/// promise. Both halves are asserted, and they are different claims: the
/// DOCUMENT is identical (what the sorter promises, and what every other pin in
/// this file is about), and the two side effects ran in the SORTED order (what
/// it explicitly does not promise). A later purity check — refusing to sort a
/// head whose values are not literals or pure paths — would turn the second
/// assertion red, which is what makes this the place the ruling lives.
///
/// A value with a side effect whose ORDER matters is the smell. The fix is to
/// lift it out of the head, which is what the book now says.
#[test]
fn sorting_an_element_head_moves_when_its_values_are_evaluated() {
    let twins = twins("evaluation-order", EVALUATION_ORDER, true);
    let written: Vec<&str> = twins.written.rendered.lines().collect();
    let sorted: Vec<&str> = twins.sorted.rendered.lines().collect();
    assert_eq!(
        written.len(),
        3,
        "the fixture prints two values and one document: {written:?}"
    );
    assert_eq!(
        (written[0], written[1]),
        ("title", "id"),
        "as WRITTEN, the values run left to right: {written:?}"
    );
    assert_eq!(
        (sorted[0], sorted[1]),
        ("id", "title"),
        "sorted, they run in the canonical order — E156's bargain, and the \
         assertion a purity check would have to come back and change: {sorted:?}"
    );
    assert_eq!(
        sort_tag_attributes(written[2]),
        sort_tag_attributes(sorted[2]),
        "the DOCUMENT moved, which the sorter does promise it never does"
    );
}

#[test]
fn the_tracked_corpus_renders_the_same_document_both_ways() {
    for relative in ELEMENT_SOURCES {
        let source = std::fs::read_to_string(repo_root().join(relative))
            .unwrap_or_else(|error| panic!("could not read {relative}: {error}"));
        let twins = twins(&relative.replace(['/', '.'], "-"), &source, false);
        assert_eq!(
            sort_tag_attributes(&twins.written.rendered),
            sort_tag_attributes(&twins.sorted.rendered),
            "{relative}: the canonical order changed what it renders"
        );
        assert_eq!(
            javascript_shape(&twins.written.javascript),
            javascript_shape(&twins.sorted.javascript),
            "{relative}: the canonical order moved more than a head"
        );
    }
}

/// The tracked fixtures must stay IN canonical order, and must still carry a
/// head worth ordering.
///
/// This is the gate that keeps the tree canonical: a head written out of order
/// in one of these files — or added to one — is red here rather than discovered
/// the next time somebody runs `vilan fmt` and gets a diff they did not ask
/// for. It doubles as the non-vacuity check for the sweep above, which would
/// otherwise pass on a file that had stopped carrying elements altogether.
#[test]
fn every_tracked_fixture_is_already_in_canonical_order() {
    for relative in ELEMENT_SOURCES {
        let source = std::fs::read_to_string(repo_root().join(relative))
            .unwrap_or_else(|error| panic!("could not read {relative}: {error}"));
        let heads = element_heads(&source);
        assert!(
            heads.iter().any(|head| head.len() >= 2),
            "{relative} no longer carries an element head of two or more items, so it proves \
             nothing. Replace it with a source that does, or drop it from ELEMENT_SOURCES."
        );
        for head in &heads {
            assert_eq!(
                element_head_permutation(head),
                None,
                "{relative} carries an element head that is not in canonical order — run \
                 `vilan fmt` on it. The head is {head:?}"
            );
        }
    }
}
