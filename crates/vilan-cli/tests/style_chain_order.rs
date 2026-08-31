//! Semantic preservation for the `style()` chain sort (kolt.local 006), proved
//! rather than argued: for a corpus of real style chains, BUILDING the source
//! and building its formatted (sorted) twin must produce the same rendered
//! style.
//!
//! Two assertions, and the first is the strong one:
//!
//!   1. **The emitted CSS is byte-identical.** Class names are content hashes of
//!      the slot key and the declaration, so this is not a weak check — if the
//!      sort changed one declaration, one selector or one condition, a hash
//!      moves and the stylesheets diverge. It holds by construction:
//!      `Style::rule` emits its atomic rule AT THE CALL, and the rule's text is
//!      a function of the slot and the declaration alone, never of the link's
//!      position in the chain.
//!
//!   2. **The surviving slot map is the same map.** The stylesheet is an
//!      over-approximation — every rule a chain builds is emitted, including one
//!      a later link overrides — so (1) alone would not catch a reorder that
//!      changed which rule WINS. The const-folded `Style` values land in the
//!      `.mjs` as literal `new Map([…])` entry lists, so the resolved slots are
//!      readable there: the two builds' JavaScript must be byte-identical once
//!      every Map's entry list is sorted. That is exactly "the same slots resolve to
//!      the same declarations, possibly inserted in a different order" — and the
//!      insertion order is the one thing a reorder is ALLOWED to change, because
//!      `class_list` joins it into a `class` attribute, which CSS reads as a set.
//!
//! If the barrier rule or the family rule were wrong, (2) is what goes red.
//!
//! The table behind the sort is gated by
//! `crates/vilan-core/tests/style_table_sync.rs`; the order itself is pinned in
//! `vilan-core`'s `formatter::style_chain_order`.

use std::path::{Path, PathBuf};

use std::process::Command;
use vilan_core::token::Token;

/// The tracked sources that carry `style()` builder chains. Each is built as
/// tracked and built again through the formatter, and the two must render the
/// same style.
///
/// These are now all IN canonical order (they were reflowed when the order
/// shipped), so the formatter is a no-op on their chains and the comparison is
/// a regression guard rather than a demonstration: it re-acquires its teeth the
/// moment a chain here is written out of order or the canonical order changes.
/// `every_tracked_fixture_is_already_in_canonical_order` is what holds them
/// canonical, and the demonstration lives in [`ORDER_SENSITIVE`] — a fixture
/// written out of order on purpose, in every shape where a wrong rule would
/// change what renders.
const STYLE_SOURCES: &[&str] = &[
    "vilan/test/style.vl",
    "vilan/test/theme.vl",
    // Both spellings in one file: the `css` blocks the block sorter orders, and
    // the chain twin they are pinned byte-identical against.
    "vilan/test/css-block.vl",
    "vilan/examples/todo/src/todos.vl",
    "vilan/examples/walkthrough/src/views.vl",
    "crates/vilan-cli/templates/browser/counter.vl",
    "crates/vilan-cli/templates/fullstack/src/client.vl",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repo root is two levels above the crate")
}

fn std_dir() -> PathBuf {
    repo_root().join("vilan/std")
}

/// The assets a build produced, split into the stylesheet(s) and everything
/// else. Each is the concatenation of the files, keyed by name and sorted, so a
/// project emitting several bundles compares as one string.
type Assets = (String, String);

/// Reads every EMITTED asset under `dir` (recursively) into `css` (stylesheets)
/// and `other` (the JavaScript bundles), each as a `--- name ---` block so the
/// comparison names the file it disagrees on.
///
/// Filtered by extension rather than by location: a bare corpus program emits
/// beside itself, a browser template emits beside its `index.html`, and a
/// fullstack project emits into `dist/`. Sources are excluded by the same
/// filter, which matters — the `.vl` is the one file the twins are SUPPOSED to
/// differ on.
fn collect(dir: &Path, prefix: &str, css: &mut Vec<String>, other: &mut Vec<String>) {
    const ASSETS: &[&str] = &["css", "js", "mjs"];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.expect("read a directory entry");
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.path().is_dir() {
            collect(&entry.path(), &format!("{name}/"), css, other);
            continue;
        }
        let Some(extension) = entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_string())
        else {
            continue;
        };
        if !ASSETS.contains(&extension.as_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let block = format!("--- {name} ---\n{text}");
        if extension == "css" {
            css.push(block);
        } else {
            other.push(block);
        }
    }
}

fn sorted_assets(css: &mut [String], other: &mut [String]) -> Assets {
    css.sort();
    other.sort();
    (other.join("\n"), css.join("\n"))
}

/// Builds the fixture at `source` and returns everything the build emitted
/// anywhere under `tree`.
///
/// A corpus program is a bare file `vilan build` compiles on its own; an example
/// or template is a PROJECT, built by pointing `vilan build` at the directory
/// holding its manifest. Both shapes are in the fixture list, so both are built
/// the way their own shape demands and then swept the same way.
fn build(source: &Path, tree: &Path, project: bool) -> Result<Assets, String> {
    let target = if project { tree } else { source };
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(target)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let (mut css, mut other) = (Vec::new(), Vec::new());
    collect(tree, "", &mut css, &mut other);
    let assets = sorted_assets(&mut css, &mut other);
    assert!(
        !assets.0.is_empty() || !assets.1.is_empty(),
        "{} built but emitted nothing to compare",
        source.display()
    );
    Ok(assets)
}

/// Copies the whole directory tree rooted at `from` into `to`. A corpus program
/// is a bare file, but an example or template is a project with a manifest and
/// siblings, so the twin has to be the whole tree.
///
/// `{{name}}` — the one placeholder `vilan init` fills in when it stamps a
/// template — is substituted on the way, so a template builds as the project it
/// becomes. Both twins get the same name, so it cannot show up as a difference.
fn copy_tree(from: &Path, to: &Path) {
    const PLACEHOLDER: &str = "{{name}}";
    std::fs::create_dir_all(to).expect("create the twin directory");
    for entry in std::fs::read_dir(from).expect("read the source directory") {
        let entry = entry.expect("read a directory entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
            continue;
        }
        match std::fs::read_to_string(entry.path()) {
            Ok(text) if text.contains(PLACEHOLDER) => {
                std::fs::write(&target, text.replace(PLACEHOLDER, "style_order_fixture"))
                    .expect("write a stamped file");
            }
            _ => {
                std::fs::copy(entry.path(), &target).expect("copy a file");
            }
        }
    }
}

/// `text` with the entry list of every `new Map([…])` sorted — the
/// canonicalization that turns "the same slots in a different insertion order"
/// into byte equality, and leaves every other difference visible.
///
/// Entries are split at the depth-zero commas of the Map's argument array, so a
/// nested list or object inside one entry travels with it.
fn sort_map_entries(text: &str) -> String {
    const OPEN: &str = "new Map([";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(&rest[..at + OPEN.len()]);
        let body = &rest[at + OPEN.len()..];
        match split_entries(body) {
            Some((entries, consumed)) => {
                let mut entries: Vec<String> = entries
                    .iter()
                    .map(|entry| sort_map_entries(entry.trim()))
                    .collect();
                entries.sort();
                out.push_str(&entries.join(", "));
                out.push(']');
                rest = &body[consumed..];
            }
            // Unbalanced (never expected from generated output): leave it alone.
            None => {
                out.push_str(body);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The depth-zero comma-separated entries of an array whose opening `[` has
/// already been consumed, plus the number of bytes consumed up to and including
/// the matching `]`. `None` when the array never closes.
fn split_entries(body: &str) -> Option<(Vec<String>, usize)> {
    let mut entries = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, character) in body.char_indices() {
        if in_string {
            current.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => {
                in_string = true;
                current.push(character);
            }
            '[' | '(' | '{' => {
                depth += 1;
                current.push(character);
            }
            ']' if depth == 0 => {
                if !current.trim().is_empty() {
                    entries.push(current);
                }
                return Some((entries, offset + 1));
            }
            ']' | ')' | '}' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => {
                entries.push(std::mem::take(&mut current));
            }
            _ => current.push(character),
        }
    }
    None
}

/// One fixture built twice — as tracked, and formatted — with both assets read
/// back. The pair is what every assertion below compares.
struct Twins {
    written: Assets,
    sorted: Assets,
}

/// The manifest directory governing `source`, if any — the directory
/// `vilan build` must be pointed at for a project fixture.
fn project_root(source: &Path, root: &Path) -> Option<PathBuf> {
    let mut directory = source.parent()?;
    loop {
        if directory.join("vilan.toml").is_file() {
            return Some(directory.to_path_buf());
        }
        if directory == root {
            return None;
        }
        directory = directory.parent()?;
    }
}

/// A scratch directory nothing else in this binary can be using.
///
/// The process id ALONE is not enough, and the failure it produced is worth
/// recording: two tests here sweep the same fixture list, nextest runs them
/// concurrently in ONE process, and the second one's `remove_dir_all` deleted
/// the tree the first was mid-build in ("copy a file: NotFound"). The counter is
/// what makes each call's tree its own.
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

fn build_both_ways(relative: &str) -> Twins {
    let root = repo_root();
    let source = root.join(relative);
    let project = project_root(&source, &root);
    // A project fixture copies its whole manifest directory; a bare corpus
    // program copies just the directory it sits in.
    let tree = project.clone().unwrap_or_else(|| {
        source
            .parent()
            .expect("a source has a parent directory")
            .to_path_buf()
    });
    let inner = source
        .strip_prefix(&tree)
        .expect("the fixture sits under its own tree");
    let temporary = scratch_directory(&format!(
        "vilan-style-order-{}",
        relative.replace(['/', '.'], "-")
    ));
    let written_dir = temporary.join("written");
    let sorted_dir = temporary.join("sorted");
    copy_tree(&tree, &written_dir);
    copy_tree(&tree, &sorted_dir);

    let written_source = written_dir.join(inner);
    let sorted_source = sorted_dir.join(inner);

    let original = std::fs::read_to_string(&sorted_source).expect("read the fixture");
    let formatted = vilan_core::formatter::format(&original);
    std::fs::write(&sorted_source, &formatted).expect("write the formatted twin");

    let is_project = project.is_some();
    let written = build(&written_source, &written_dir, is_project)
        .unwrap_or_else(|error| panic!("{relative} did not build as tracked:\n{error}"));
    let sorted = build(&sorted_source, &sorted_dir, is_project)
        .unwrap_or_else(|error| panic!("{relative} did not build once formatted:\n{error}"));
    let _ = std::fs::remove_dir_all(&temporary);

    Twins { written, sorted }
}

/// The headline invariant. Sorting a chain cannot change one byte of the
/// stylesheet it renders.
#[test]
fn sorting_a_style_chain_leaves_the_emitted_css_byte_identical() {
    for relative in STYLE_SOURCES {
        let twins = build_both_ways(relative);
        assert_eq!(
            twins.written.1, twins.sorted.1,
            "{relative}: the canonical order changed the emitted CSS. Class names are content \
             hashes of the slot and the declaration, so a moved hash means a chain rendered \
             differently — the barrier rule or the family rule let two dependent slots cross."
        );
    }
}

/// The resolution invariant: the same slots survive, with the same declarations.
/// Only their insertion order — the order `class_list` joins them into a `class`
/// attribute, which CSS reads as a set — may differ.
#[test]
fn sorting_a_style_chain_resolves_the_same_slots() {
    for relative in STYLE_SOURCES {
        let twins = build_both_ways(relative);
        let written = sort_map_entries(&twins.written.0);
        let sorted = sort_map_entries(&twins.sorted.0);
        assert_eq!(
            written, sorted,
            "{relative}: the canonical order changed which slots resolve, or what they resolve \
             to — not merely the order they were inserted in. A reorder crossed two dependent \
             slots (last-wins per slot, and per FAMILY through `without_covered`)."
        );
    }
}

/// The number of `.name(…)` links in the longest `style()` builder chain in
/// `source` — enough to tell a fixture that exercises the machinery from one
/// that no longer carries a chain at all.
fn longest_chain(source: &str) -> usize {
    let (tokens, errors) = vilan_core::lexing::tokenize(source);
    assert!(errors.is_empty(), "a fixture did not lex");
    let tokens: Vec<_> = tokens.into_iter().map(|(token, _)| token).collect();
    let mut longest = 0;
    for (at, token) in tokens.iter().enumerate() {
        if !matches!(token, Token::Ident("style"))
            || !matches!(tokens.get(at + 1), Some(Token::Ctrl('(')))
            || !matches!(tokens.get(at + 2), Some(Token::Ctrl(')')))
        {
            continue;
        }
        let mut links = 0;
        let mut cursor = at + 3;
        while matches!(tokens.get(cursor), Some(Token::Ctrl('.')))
            && matches!(tokens.get(cursor + 1), Some(Token::Ident(_)))
            && matches!(tokens.get(cursor + 2), Some(Token::Ctrl('(')))
        {
            links += 1;
            let mut depth = 0usize;
            cursor += 2;
            while let Some(token) = tokens.get(cursor) {
                match token {
                    Token::Ctrl('(') | Token::Ctrl('[') | Token::Ctrl('{') => depth += 1,
                    Token::Ctrl(')') | Token::Ctrl(']') | Token::Ctrl('}') => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            cursor += 1;
        }
        longest = longest.max(links);
    }
    longest
}

/// The tracked fixtures must stay IN canonical order, and must still carry a
/// chain worth ordering.
///
/// This is the gate that keeps the tree canonical: a chain written out of order
/// in one of these files — or added to one — is red here rather than discovered
/// the next time somebody runs `vilan fmt` and gets a diff they did not ask for.
/// It doubles as the non-vacuity check for the two invariants above, which would
/// otherwise pass on a file that had stopped carrying chains altogether.
#[test]
fn every_tracked_fixture_is_already_in_canonical_order() {
    for relative in STYLE_SOURCES {
        let source = std::fs::read_to_string(repo_root().join(relative))
            .unwrap_or_else(|error| panic!("could not read {relative}: {error}"));
        let links = longest_chain(&source);
        assert!(
            links >= 2,
            "{relative} no longer carries a `style()` chain of two or more links, so it proves \
             nothing. Replace it with a source that does, or drop it from STYLE_SOURCES."
        );
        let (tokens, errors) = vilan_core::lexing::tokenize(&source);
        assert!(errors.is_empty(), "{relative} did not lex");
        let plain: Vec<_> = tokens.into_iter().map(|(token, _)| token).collect();
        assert_eq!(
            vilan_core::formatter::sort_style_chains(plain.clone()),
            plain,
            "{relative} carries a `style()` chain that is not in canonical order — run \
             `vilan fmt` on it."
        );
        // The block spelling's half of the same gate (css-block.md §8, S3).
        assert_eq!(
            vilan_core::formatter::sort_css_blocks(plain.clone()),
            plain,
            "{relative} carries a `css` block whose items are not in canonical order — run \
             `vilan fmt` on it."
        );
    }
}

/// The demonstration the tracked fixtures can no longer be, now that they are
/// canonical: chains written OUT of order on purpose, built both ways.
///
/// Two halves. The first is adversarial — every shape where a wrong FAMILY or a
/// missing BARRIER would silently change what renders:
///
///   * a longhand written after its shorthand, and before it (`padding`);
///   * the `size` pair, which writes the same two slots `width`/`height` do;
///   * `border_color` on either side of the `border` shorthand that covers it;
///   * two methods on one slot (`line_height` / `line_height_length`);
///   * a slot-writing escape hatch (`raw`) whose property is a shorthand over a
///     longhand beside it — only the barrier rule keeps them from crossing.
///
/// The second half is the tracked corpus's own chains in their PRE-REFLOW
/// spelling, so the real shapes stay exercised unsorted after the tree was
/// canonicalized: `todos.vl`'s shell and button, `views.vl`'s shell, and the
/// project templates' card.
const ORDER_SENSITIVE: &str = concat!(
    "import std::io::print;\n",
    "import std::style::{ AlignItems, Color, Cursor, Display, FlexDirection, Length, space, style };\n",
    "\n",
    "fun main() {\n",
    // --- the adversarial half ---
    // The shorthand LAST wins the whole box; the longhand last wins one edge.
    // Reversing either changes what renders.
    "\tlet a = const style().color(Color::gray(900)).padding_left(space(1)).padding(space(4));\n",
    "\tlet b = const style().color(Color::gray(900)).padding(space(4)).padding_left(space(1));\n",
    // `size` writes width and height; `width` writes one of them.
    "\tlet c = const style().gap(space(2)).size(Length::px(16)).width(Length::px(32));\n",
    "\tlet d = const style().gap(space(2)).width(Length::px(32)).size(Length::px(16));\n",
    // `border-color` is one of `border`'s longhands.
    "\tlet e = const style().display(Display::Flex).border_color(Color::gray(300))",
    ".border(Length::px(1), Color::gray(500));\n",
    "\tlet f = const style().display(Display::Flex).border(Length::px(1), Color::gray(500))",
    ".border_color(Color::gray(300));\n",
    // Two methods, one slot.
    "\tlet g = const style().margin(space(1)).line_height_length(Length::px(24)).line_height(1.5);\n",
    // A `raw` whose slot is a SHORTHAND over the longhand written before it. As
    // written, the `raw` clears the `padding_top`; let the sort move it anywhere
    // else and the `padding_top` survives instead.
    "\tlet h = const style().padding_top(space(1)).raw(\"padding\", \"9px\")",
    ".display(Display::Flex).color(Color::gray(900));\n",
    // …and the mirror, hatch first: the `padding_top` must stay AFTER it, or the
    // shorthand stops being overridden on that edge.
    "\tlet i = const style().raw(\"padding\", \"9px\").padding_top(space(1))",
    ".display(Display::Flex).color(Color::gray(900));\n",
    // --- the tracked corpus's chains, as they were written before the reflow ---
    "\tlet shell = const style().max_width(Length::rem(32)).margin_x(Length::auto())",
    ".padding_x(space(4)).padding_y(space(8)).font_size(Length::px(16)).line_height(1.5)",
    ".font_family(\"system-ui, sans-serif\");\n",
    "\tlet button = const style().padding_x(space(2)).padding_y(space(1)).radius(space(1))",
    ".border(Length::px(1), Color::gray(300)).background(Color::gray(50)).cursor(Cursor::Pointer)",
    ".hover(style().background(Color::gray(100)));\n",
    "\tlet panel = const style().max_width(Length::rem(30)).margin_x(Length::auto())",
    ".padding(space(6)).display(Display::Flex).flex_direction(FlexDirection::Column)",
    ".gap(space(3)).align_items(AlignItems::Center).font_family(\"system-ui, sans-serif\");\n",
    "\tlet card = const style().display(Display::Flex).flex_direction(FlexDirection::Column)",
    ".gap(space(3)).padding(space(4)).radius(space(2)).background(Color::gray(100));\n",
    "\tprint(a.class_list());\n",
    "\tprint(b.class_list());\n",
    "\tprint(c.class_list());\n",
    "\tprint(d.class_list());\n",
    "\tprint(e.class_list());\n",
    "\tprint(f.class_list());\n",
    "\tprint(g.class_list());\n",
    "\tprint(h.class_list());\n",
    "\tprint(i.class_list());\n",
    "\tprint(shell.class_list());\n",
    "\tprint(button.class_list());\n",
    "\tprint(panel.class_list());\n",
    "\tprint(card.class_list());\n",
    "}\n",
    "main();\n",
);

/// One deliberately-unsorted fixture, built as written and built formatted.
/// Refuses to run on a fixture the formatter leaves alone — a fixture that does
/// not reorder proves nothing.
fn order_sensitive_twins(label: &str, fixture: &str) -> Twins {
    let temporary = scratch_directory(&format!("vilan-style-order-sensitive-{label}"));
    let written_dir = temporary.join("written");
    let sorted_dir = temporary.join("sorted");
    std::fs::create_dir_all(&written_dir).expect("create the written directory");
    std::fs::create_dir_all(&sorted_dir).expect("create the sorted directory");
    let written_source = written_dir.join("sensitive.vl");
    let sorted_source = sorted_dir.join("sensitive.vl");
    let formatted = vilan_core::formatter::format(fixture);
    assert_ne!(
        formatted, fixture,
        "the {label} order-sensitive fixture did not reorder, so it proves nothing"
    );
    std::fs::write(&written_source, fixture).expect("write the fixture");
    std::fs::write(&sorted_source, &formatted).expect("write the sorted twin");

    let written = build(&written_source, &written_dir, false).unwrap_or_else(|error| {
        panic!("the {label} order-sensitive fixture did not build:\n{error}")
    });
    let sorted = build(&sorted_source, &sorted_dir, false)
        .unwrap_or_else(|error| panic!("the {label} sorted twin did not build:\n{error}"));
    let _ = std::fs::remove_dir_all(&temporary);
    Twins { written, sorted }
}

#[test]
fn an_order_sensitive_fixture_resolves_the_same_slots() {
    let twins = order_sensitive_twins("chain", ORDER_SENSITIVE);
    assert_eq!(
        twins.written.1, twins.sorted.1,
        "the canonical order changed the emitted CSS of the order-sensitive fixture"
    );
    assert_eq!(
        sort_map_entries(&twins.written.0),
        sort_map_entries(&twins.sorted.0),
        "the canonical order changed which slots resolve in the order-sensitive fixture — a \
         dependent pair crossed. This is the fixture the family and barrier rules exist for."
    );
}

/// The same demonstration for a `css` BLOCK (proposal/css-block.md §8, S3). The
/// block sorts by the same order function, reading the same tables through the
/// CSS property names its declarations write — so it needs the same proof, in
/// the same adversarial shapes, or the derivation from `properties` to `family`
/// would be an argument rather than a fact.
///
/// The one shape the chain fixture cannot carry: in a chain, `raw` is a BARRIER,
/// so nothing crosses a shorthand written as `.raw("padding", …)`. In a block
/// the property is a token, so `padding:` really does rank — which is what makes
/// the entangled-pair cases below load-bearing here in a way they are not there.
const CSS_ORDER_SENSITIVE: &str = concat!(
    "import std::io::print;\n",
    "import std::style::{ Color, Length, space, style };\n",
    "\n",
    "fun main() {\n",
    // The shorthand LAST wins the whole box; the longhand last wins one edge.
    // Both must survive the sort, and only the family rule makes them.
    "\tlet a = const css { color: {Color::gray(900)}; padding-left: {space(1)}; ",
    "padding: {space(4)}; };\n",
    "\tlet b = const css { color: {Color::gray(900)}; padding: {space(4)}; ",
    "padding-left: {space(1)}; };\n",
    // `size` writes width and height; `width` writes one of them. In a block
    // there is no `size` property, so the pair is `width`/`height` themselves.
    "\tlet c = const css { gap: {space(2)}; width: 32px; height: 16px; };\n",
    // `border-color` is one of `border`'s longhands.
    "\tlet e = const css { display: flex; border-color: {Color::gray(300)}; ",
    "border: 1px solid {Color::gray(500)}; };\n",
    "\tlet f = const css { display: flex; border: 1px solid {Color::gray(500)}; ",
    "border-color: {Color::gray(300)}; };\n",
    // A property the table does not write is a BARRIER: `padding-top` must not
    // cross it to reach `display`, or the vendor rule stops landing where it was
    // written.
    "\tlet h = const css { padding-top: {space(1)}; -webkit-mask-composite: source-in; ",
    "display: flex; color: {Color::gray(900)}; };\n",
    // Conditions sort after every declaration, and among themselves by axis —
    // media, relation, attribute, pseudo — which is the order the selector nests
    // them in, so a wrong axis would change what the rule matches.
    "\tlet i = const css {\n",
    "\t\t.hover { color: {Color::gray(50)}; }\n",
    "\t\tpadding: {space(2)};\n",
    "\t\t.within(\"data-theme\", \"dark\") { color: {Color::gray(100)}; }\n",
    "\t\tdisplay: flex;\n",
    "\t\t.md { padding: {space(6)}; }\n",
    "\t};\n",
    // A nested rule's own body sorts too, with the same rules inside it.
    "\tlet j = const css {\n",
    "\t\t.hover { padding-left: {space(1)}; padding: {space(4)}; display: flex; }\n",
    "\t};\n",
    "\tprint(a.class_list());\n",
    "\tprint(b.class_list());\n",
    "\tprint(c.class_list());\n",
    "\tprint(e.class_list());\n",
    "\tprint(f.class_list());\n",
    "\tprint(h.class_list());\n",
    "\tprint(i.class_list());\n",
    "\tprint(j.class_list());\n",
    "}\n",
    "main();\n",
);

#[test]
fn an_order_sensitive_css_block_resolves_the_same_slots() {
    let twins = order_sensitive_twins("css-block", CSS_ORDER_SENSITIVE);
    assert_eq!(
        twins.written.1, twins.sorted.1,
        "the canonical order changed the emitted CSS of the order-sensitive `css` block fixture. \
         Class names are content hashes of the slot and the declaration, so a moved hash means a \
         block rendered differently."
    );
    assert_eq!(
        sort_map_entries(&twins.written.0),
        sort_map_entries(&twins.sorted.0),
        "the canonical order changed which slots resolve in the order-sensitive `css` block \
         fixture — a dependent pair crossed. The block ranks a declaration by the CSS PROPERTY it \
         writes, so this is where the `properties`-to-`family` derivation is proved."
    );
}

/// The `.mjs` canonicalization must not be able to hide a real difference: a
/// changed class name, declaration or slot key survives it.
#[test]
fn the_map_canonicalization_still_sees_a_real_change() {
    let one = "const a = [ new Map([ [ \"x\", [ \"c1\", \"color:red\" ] ], [ \"y\", [ \"c2\", \"gap:1\" ] ] ]) ];";
    let reordered = "const a = [ new Map([ [ \"y\", [ \"c2\", \"gap:1\" ] ], [ \"x\", [ \"c1\", \"color:red\" ] ] ]) ];";
    let altered = "const a = [ new Map([ [ \"x\", [ \"c1\", \"color:blue\" ] ], [ \"y\", [ \"c2\", \"gap:1\" ] ] ]) ];";
    assert_eq!(
        sort_map_entries(one),
        sort_map_entries(reordered),
        "a pure reorder should canonicalize equal"
    );
    assert_ne!(
        sort_map_entries(one),
        sort_map_entries(altered),
        "a changed declaration must survive the canonicalization"
    );
}

/// The sort's boundary, on the far side of which reordering would be WRONG
/// (kolt.local 032). A `style()` chain may be permuted because each link owns a
/// slot and `Style::rule` emits the rule AT THE CALL, independent of position —
/// which is what the two headline tests above prove. A `declarations()` chain is
/// the opposite: its links are cascade TEXT, joined in authoring order into one
/// block, so permuting them changes what the block declares.
///
/// What defends it is the root gate — `starts_style_builder` requires the
/// literal `style ( )` token run — and this pin is what keeps that from being an
/// accident. The fixture is TOKEN TEXT rather than a compiling program, because
/// the sort is a token pass and the gate is the whole subject: the two chains
/// below carry the SAME link names in the same non-canonical order, so the
/// `style()` one must permute and the `declarations()` one must not. If the two
/// ever agree, the gate has gone.
#[test]
fn a_declarations_chain_is_never_reordered_by_the_style_chain_sort() {
    let links = ".padding(space(4)).display(Display::Flex)";
    let permuted = |root: &str| {
        let source = format!("let s = const {root}(){links};\n");
        let (tokens, errors) = vilan_core::lexing::tokenize(&source);
        assert!(errors.is_empty(), "the {root} fixture did not lex");
        let plain: Vec<Token> = tokens.into_iter().map(|(token, _)| token).collect();
        vilan_core::formatter::sort_style_chains(plain.clone()) != plain
    };
    assert!(
        permuted("style"),
        "the fixture must be out of canonical order, or this pin passes for the wrong reason"
    );
    assert!(
        !permuted("declarations"),
        "the style-chain sort reached a `declarations()` chain — its links are cascade text \
         joined in authoring order, so permuting them changes what the block declares."
    );
}
