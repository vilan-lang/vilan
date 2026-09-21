//! The gate that holds [`crate::html_attributes`] to its dataset (tracker E69).
//!
//! The lineage, and the one thing that matters about it — each arrow has
//! exactly one implementation in the place that runs it:
//!
//! ```text
//!   the WHATWG + SVG indices
//!        --scripts/regen-html-attributes.py --fetch-->  html-attributes.tsv
//!        --this file-->                                 html_attributes.rs
//! ```
//!
//! The left arrow runs rarely (when a spec publishes) and needs the network;
//! the right arrow is this file, runs on every `cargo test`, and needs nothing
//! — which is what makes "generated AND gated" true offline.
//!
//! **One implementation each, and that is newer than the diagram** (tracker
//! N107). The python script carried a second implementation of the right arrow
//! — a default mode that rendered the TSV into the table, for the convenience
//! of refreshing both files in one command — and this gate held the two equal
//! byte for byte, so they could not drift in silence. Held equal is not the
//! same as not duplicated: two renderers of one table is two places to edit
//! when a column moves, and one of them runs on every `cargo test` while the
//! other runs when somebody remembers. The convenience is kept without the
//! twin: [`REGENERATE_ENV`] is the rewrite, and the script's last line points
//! at it.
//!
//! Four checks:
//!
//!   1. **The table is the dataset** — the whole file is re-rendered from the
//!      TSV and compared BYTE FOR BYTE. [`REGENERATE_ENV`] rewrites it instead
//!      of failing. This is the only check that has to pass for the table to be
//!      honest; the rest exist to catch a bad TSV.
//!   2. **Provenance** — the TSV header names both spec URLs and a sha256 for
//!      each, so a refresh that forgot to record what it read is a red.
//!   3. **The shape is offerable** — every name in the table is a name a vilan
//!      element head can actually spell (`parsing.rs::parse_element_name`: a
//!      name, then hyphen-joined names), and no event name still carries its
//!      `on` prefix.
//!   4. **The exhibits survive a refresh** — `input` keeps `type`, `disabled`
//!      and `value`, `svg` keeps `viewBox`, the globals keep `class` and `id`,
//!      and the events keep `click`. A spec reorganization that loses them is
//!      a red here rather than a silently emptier popup.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// The dataset, relative to the crate root.
const DATASET: &str = "src/html-attributes.tsv";

/// The generated table this gate holds to it.
const TABLE: &str = "src/html_attributes.rs";

/// Set (to anything) to make [`the_generated_table_is_the_dataset`] REWRITE the
/// stale table in place instead of failing — the regeneration entry point.
const REGENERATE_ENV: &str = "VILAN_REGENERATE_HTML_ATTRIBUTES";

/// The regeneration command, verbatim — named by every red this file raises.
const REGENERATE_COMMAND: &str =
    "VILAN_REGENERATE_HTML_ATTRIBUTES=1 cargo test -p vilan-ide html_attributes";

/// The `element` column's two non-tag keys.
const HTML_GLOBAL: &str = "*";
const SVG_GLOBAL: &str = "svg:*";

/// The `source` column's vocabulary.
const SOURCE_HTML_EVENT: &str = "html-event-handler-index";
const SOURCE_SVG_ELEMENT: &str = "svg-element-index";

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// One dataset row: `(element, attribute, source)`.
fn dataset() -> Vec<(String, String, String)> {
    let path = crate_root().join(DATASET);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        let element = columns.next().expect("a row has an element column");
        let attribute = columns
            .next()
            .unwrap_or_else(|| panic!("no attribute in `{line}`"));
        let source = columns
            .next()
            .unwrap_or_else(|| panic!("no source in `{line}`"));
        assert!(
            columns.next().is_none(),
            "a dataset row has exactly three columns: `{line}`"
        );
        rows.push((
            element.to_string(),
            attribute.to_string(),
            source.to_string(),
        ));
    }
    assert!(!rows.is_empty(), "{DATASET} has no rows");
    rows
}

/// The dataset's header lines, `#` and its space stripped.
fn header() -> Vec<String> {
    let path = crate_root().join(DATASET);
    std::fs::read_to_string(&path)
        .expect("the dataset is readable")
        .lines()
        .take_while(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .collect()
}

/// A `pub const NAME: &[&str] = &[…];` block, one value per line.
fn rust_list(name: &str, doc: &[&str], values: &BTreeSet<String>) -> Vec<String> {
    let mut out: Vec<String> = doc.iter().map(|line| (*line).to_string()).collect();
    out.push(format!("pub const {name}: &[&str] = &["));
    out.extend(values.iter().map(|value| format!("    \"{value}\",")));
    out.push("];".to_string());
    out
}

/// The whole of `html_attributes.rs`, rendered from the dataset.
///
/// The ONE implementation of the TSV -> table arrow (N107). It mirrored the
/// python generator's `render_table` line for line, with the byte comparison
/// below keeping the two honest about it; the python half is gone and this is
/// what a refresh runs through.
fn render_table(rows: &[(String, String, String)]) -> String {
    let mut globals = BTreeSet::new();
    let mut events = BTreeSet::new();
    let mut svg_globals = BTreeSet::new();
    let mut svg_elements = BTreeSet::new();
    let mut per_element: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (element, attribute, source) in rows {
        if source == SOURCE_SVG_ELEMENT {
            svg_elements.insert(element.clone());
            per_element.entry(element.clone()).or_default();
        } else if source == SOURCE_HTML_EVENT {
            events.insert(attribute.clone());
        } else if element == HTML_GLOBAL {
            globals.insert(attribute.clone());
        } else if element == SVG_GLOBAL {
            svg_globals.insert(attribute.clone());
        } else {
            per_element
                .entry(element.clone())
                .or_default()
                .insert(attribute.clone());
        }
    }
    // `onclick` is the CONTENT attribute; vilan writes `on:click(…)`, so the
    // table holds what follows the colon.
    let events: BTreeSet<String> = events
        .iter()
        .map(|event| event.strip_prefix("on").unwrap_or(event).to_string())
        .collect();

    let mut lines: Vec<String> = [
        "//! Every attribute name the HTML and SVG specifications define, by element",
        "//! — the vocabulary vilan-ide's element-head completion offers at `<tag |>`",
        "//! (tracker E69).",
        "//!",
        "//! GENERATED FILE -- do not hand-edit. The dataset is `html-attributes.tsv`",
        "//! beside this file, vendored from the WHATWG HTML attribute index and the",
        "//! SVG 2 attribute index by `scripts/regen-html-attributes.py --fetch`.",
        "//! `html_attributes_sync` beside this file renders the TSV into it and diffs",
        "//! the result byte for byte on every `cargo test` — the ONE implementation of",
        "//! that arrow, and the gate (tracker N107). It is OFFLINE, so no gate ever",
        "//! reaches the network and refreshing the extract stays a deliberate commit.",
        "//!",
        "//! Rewrite after refreshing the dataset with",
        "//! `VILAN_REGENERATE_HTML_ATTRIBUTES=1 cargo test -p vilan-ide html_attributes`.",
        "//!",
        "//! # What this table is NOT",
        "//!",
        "//! It is not a source of truth about vilan. The element desugar stays",
        "//! NAME-BLIND (element-syntax.md §2, §9 item 3): `name(x)` lowers to",
        "//! `.attr(\"name\", x)` whatever `name` is, nothing here is consulted by the",
        "//! parser, the desugar, the analyzer or the formatter, and a `data-*` name",
        "//! absent from it costs one completion entry and nothing else. That is the",
        "//! disanalogy E67's refusal turned on — \"a second source of truth with",
        "//! nothing to gate it\" — and the generator plus the gate are that gate.",
        "",
    ]
    .iter()
    .map(|line| (*line).to_string())
    .collect();

    lines.extend(rust_list(
        "GLOBAL_ATTRIBUTES",
        &[
            "/// The global attributes: every element takes them (the HTML attribute",
            "/// index's `HTML elements` rows).",
        ],
        &globals,
    ));
    lines.push(String::new());
    lines.extend(rust_list(
        "SVG_GLOBAL_ATTRIBUTES",
        &[
            "/// The SVG-wide vocabulary — the attribute index's section G.2",
            "/// presentation attributes — offered on every [`SVG_ELEMENTS`] tag.",
        ],
        &svg_globals,
    ));
    lines.push(String::new());
    lines.extend(rust_list(
        "EVENTS",
        &[
            "/// The `GlobalEventHandlers` event names with `on` stripped: vilan spells",
            "/// the content attribute `onclick` as `on:click(…)`, so the table holds",
            "/// what follows the colon. The `body`/`Window` handlers are a different",
            "/// mixin and are not here.",
        ],
        &events,
    ));
    lines.push(String::new());
    lines.extend(rust_list(
        "SVG_ELEMENTS",
        &[
            "/// The SVG-shaped tags: every element the SVG index names that the HTML",
            "/// index does not, plus the SVG root. These take",
            "/// [`SVG_GLOBAL_ATTRIBUTES`] on top of their own.",
        ],
        &svg_elements,
    ));
    lines.push(String::new());
    lines.extend(
        [
            "/// Each tag's OWN attributes as `(tag, attribute)` pairs, ascending — the",
            "/// globals above are not repeated, so a tag absent from this table takes",
            "/// only the globals. Sorted, so a lookup is a binary search and a diff of",
            "/// this file reads as a set difference; flat rather than nested so that",
            "/// every line is short enough for `cargo fmt` to leave it exactly where the",
            "/// generator put it (the gate compares the file BYTE for byte).",
            "pub const ELEMENT_ATTRIBUTES: &[(&str, &str)] = &[",
        ]
        .iter()
        .map(|line| (*line).to_string()),
    );
    for (element, attributes) in &per_element {
        let wide = if svg_elements.contains(element) {
            &svg_globals
        } else {
            &BTreeSet::new()
        };
        for attribute in attributes {
            if globals.contains(attribute) || wide.contains(attribute) {
                continue;
            }
            lines.push(format!("    (\"{element}\", \"{attribute}\"),"));
        }
    }
    lines.push("];".to_string());
    lines.push(String::new());
    lines.join("\n")
}

/// Check 1, and the regeneration entry point.
#[test]
fn the_generated_table_is_the_dataset() {
    let path = crate_root().join(TABLE);
    let rendered = render_table(&dataset());
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current == rendered {
        return;
    }
    if std::env::var_os(REGENERATE_ENV).is_some() {
        std::fs::write(&path, &rendered).expect("the table is writable");
        return;
    }
    let first_difference = current
        .lines()
        .zip(rendered.lines())
        .position(|(left, right)| left != right)
        .map(|line| {
            format!(
                "line {}:\n  table:   {:?}\n  dataset: {:?}",
                line + 1,
                current.lines().nth(line).unwrap_or(""),
                rendered.lines().nth(line).unwrap_or(""),
            )
        })
        .unwrap_or_else(|| {
            format!(
                "the table has {} lines, the dataset derives {}",
                current.lines().count(),
                rendered.lines().count()
            )
        });
    panic!(
        "{TABLE} is not what {DATASET} derives — {first_difference}\n\
         regenerate with `{REGENERATE_COMMAND}`"
    );
}

/// Check 2: a refresh that did not record what it read is not a refresh.
#[test]
fn the_dataset_records_its_provenance() {
    let header = header();
    assert!(
        header.iter().any(|line| line.starts_with("fetched:")),
        "the dataset header names no fetch date: {header:?}"
    );
    for url in [
        "https://html.spec.whatwg.org/multipage/indices.html",
        "https://www.w3.org/TR/SVG2/attindex.html",
    ] {
        assert!(
            header.iter().any(|line| line.contains(url)),
            "the dataset header does not name {url}"
        );
    }
    let hashes = header
        .iter()
        .filter(|line| line.starts_with("sha256 "))
        .count();
    assert_eq!(hashes, 2, "one content hash per source page: {header:?}");
}

/// Check 3: every offered name is a name a head can spell.
#[test]
fn every_offered_name_is_spellable_in_an_element_head() {
    let spellable = |name: &str| {
        !name.is_empty()
            && !name.starts_with('-')
            && !name.ends_with('-')
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            && !name.as_bytes()[0].is_ascii_digit()
    };
    let named = crate::html_attributes::GLOBAL_ATTRIBUTES
        .iter()
        .chain(crate::html_attributes::SVG_GLOBAL_ATTRIBUTES)
        .chain(crate::html_attributes::EVENTS)
        .chain(crate::html_attributes::SVG_ELEMENTS)
        .copied()
        .chain(
            crate::html_attributes::ELEMENT_ATTRIBUTES
                .iter()
                .flat_map(|(element, attribute)| [*element, *attribute]),
        );
    for name in named {
        assert!(spellable(name), "`{name}` has no element-head spelling");
    }
    for event in crate::html_attributes::EVENTS {
        assert!(
            !event.starts_with("on"),
            "`{event}` still carries the content attribute's `on` — `on:{event}` would read `on:on…`"
        );
    }
}

/// Check 4: the exhibits the item named, so a spec reorganization that loses
/// them is a red rather than an emptier popup.
#[test]
fn the_tables_still_carry_their_exhibits() {
    let own = |tag: &str, attribute: &str| {
        crate::html_attributes::ELEMENT_ATTRIBUTES
            .binary_search(&(tag, attribute))
            .is_ok()
    };
    for attribute in ["type", "disabled", "value"] {
        assert!(own("input", attribute), "`input` lost `{attribute}`");
    }
    assert!(own("svg", "viewBox"), "`svg` lost `viewBox`");
    assert!(
        crate::html_attributes::SVG_ELEMENTS.contains(&"svg"),
        "`svg` is no longer SVG-shaped, so the presentation attributes are lost with it"
    );
    for attribute in ["class", "id"] {
        assert!(
            crate::html_attributes::GLOBAL_ATTRIBUTES.contains(&attribute),
            "the globals lost `{attribute}`"
        );
    }
    for attribute in ["fill", "stroke", "stroke-width"] {
        assert!(
            crate::html_attributes::SVG_GLOBAL_ATTRIBUTES.contains(&attribute),
            "the SVG-wide vocabulary lost `{attribute}` (lucide's own shape)"
        );
    }
    for event in ["click", "input", "submit"] {
        assert!(
            crate::html_attributes::EVENTS.contains(&event),
            "the events lost `{event}`"
        );
    }
}

/// The table is searched with `binary_search`, which is only an answer if it is
/// sorted — and `cargo fmt` is not what keeps it that way.
#[test]
fn the_table_is_sorted() {
    assert!(
        crate::html_attributes::ELEMENT_ATTRIBUTES.is_sorted(),
        "ELEMENT_ATTRIBUTES is searched by binary_search"
    );
    for table in [
        crate::html_attributes::GLOBAL_ATTRIBUTES,
        crate::html_attributes::SVG_GLOBAL_ATTRIBUTES,
        crate::html_attributes::EVENTS,
        crate::html_attributes::SVG_ELEMENTS,
    ] {
        assert!(table.is_sorted(), "{table:?} is not sorted");
    }
}
