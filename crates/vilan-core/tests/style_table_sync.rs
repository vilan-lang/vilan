//! The canonical-order table, held to `vilan/std/src/style.vl`.
//!
//! `vilan fmt` sorts a `style()` builder chain by a table of method names
//! (`formatter::STYLE_PROPERTY_METHODS` and friends). A table that drifts from
//! the std surface it describes is worse than no table: a method it has never
//! heard of becomes a BARRIER, so the drift shows up as a chain that quietly
//! stops sorting rather than as a failure. This file is the gate — the
//! `grammar_sync` shape (Order 11): the source of truth is read out of the real
//! artefact, and the hand-written table is held to it in BOTH directions.
//!
//! Four gates, each derived from `style.vl` rather than restated:
//!
//!   1. **Completeness** — every `fun name(self, …)` in an `impl Style` block is
//!      claimed by exactly one of the three tables, and every table row names a
//!      method that exists. A new style method is red until it is placed.
//!   2. **Slots** — each property row's `properties` column is derived from the
//!      method's BODY (the `raw` / `with_length` / `with_color` / `with_border`
//!      / `rule` call it makes) and must match what the table claims.
//!   3. **Families** — the `family` column must be exactly the partition induced
//!      by slot entanglement, where "entangled" is read from `style.vl`'s own
//!      `family_longhands` shorthand table. Asserted in both directions: two
//!      entangled methods MUST share a family, and a family's members MUST be
//!      connected by entanglement (so the column cannot over-group either, which
//!      would silently stop the sort doing its work).
//!   4. **Condition axes** — a condition that delegates to another condition
//!      (`hover` is `pseudo("hover", …)`, `md` is `media("48rem", …)`) must be
//!      recorded on the axis it delegates to.
//!
//! The behaviour the table drives is pinned in `formatter.rs`'s
//! `mod style_chain_order`; the proof that a reorder preserves the rendered
//! style is `crates/vilan-cli/tests/style_chain_order.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use vilan_core::formatter::{
    ConditionAxis, STYLE_BARRIER_METHODS, STYLE_CONDITION_METHODS, STYLE_PROPERTY_METHODS,
};

/// The std style surface this table describes.
const STYLE_SOURCE: &str = "vilan/std/src/style.vl";

/// The five `Style` methods that actually write a slot. Every other property
/// method is spelled in terms of one of them, which is what makes the slot a
/// method body can write DERIVABLE: the property is the first argument, except
/// for the `rule` chokepoint where it is the third.
const SLOT_WRITERS: &[(&str, usize)] = &[
    ("raw", 0),
    ("with_length", 0),
    ("with_color", 0),
    ("with_border", 0),
    ("rule", 2),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repo root is two levels above the crate")
}

fn style_source() -> String {
    let path = repo_root().join(STYLE_SOURCE);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("could not read {}: {error}", path.display());
    })
}

/// The lines of the top-level item that begins at the line satisfying `opens`,
/// up to the closing `}` in column zero. `style.vl` indents with tabs, so a
/// top-level item is exactly the span between its header and the first
/// unindented `}`.
fn top_level_block(source: &str, opens: impl Fn(&str) -> bool) -> Vec<&str> {
    let mut lines = source.lines();
    let mut block = Vec::new();
    let mut inside = false;
    for line in lines.by_ref() {
        if !inside {
            inside = opens(line);
            continue;
        }
        if line == "}" {
            return block;
        }
        block.push(line);
    }
    assert!(!block.is_empty(), "no block opened, or none closed");
    block
}

/// Every `fun name(self, …)` declared in an `impl Style` block, in source order,
/// paired with the raw lines of its body. Both blocks are read (`impl Style` and
/// `impl Style with Add`) so that a method landing in either one is caught.
fn declared_methods(source: &str) -> Vec<(String, Vec<String>)> {
    let mut methods = Vec::new();
    for header in ["impl Style {", "impl Style with Add {"] {
        let block = top_level_block(source, |line| line == header);
        let mut current: Option<(String, Vec<String>)> = None;
        for line in block {
            let trimmed = line.trim_start_matches('\t');
            let depth = line.len() - trimmed.len();
            if depth == 1 && trimmed.starts_with("fun ") {
                if let Some(method) = current.take() {
                    methods.push(method);
                }
                let name = trimmed["fun ".len()..]
                    .split('(')
                    .next()
                    .expect("a `fun` header names something")
                    .to_string();
                let takes_self = trimmed
                    .split_once('(')
                    .is_some_and(|(_, rest)| rest.starts_with("self"));
                current = takes_self.then(|| (name, Vec::new()));
            } else if let Some((_, body)) = current.as_mut() {
                // Kept untrimmed: the delegation pin reads a statement's depth.
                body.push(line.to_string());
            }
        }
        if let Some(method) = current.take() {
            methods.push(method);
        }
    }
    assert!(
        methods.len() > 60,
        "suspiciously few `Style` methods parsed out of {STYLE_SOURCE}: {}",
        methods.len()
    );
    methods
}

/// Splits an argument list at depth-zero commas. `text` is everything after the
/// opening parenthesis; parsing stops at the matching close.
fn arguments(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for character in text.chars() {
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
            '(' | '[' | '{' => {
                depth += 1;
                current.push(character);
            }
            ')' if depth == 0 => {
                parts.push(current.trim().to_string());
                return parts;
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    parts.push(current.trim().to_string());
    parts
}

/// A string literal's contents, or `None` when `text` is any other expression.
fn string_literal(text: &str) -> Option<&str> {
    text.strip_prefix('"')?.strip_suffix('"')
}

/// The CSS properties a method body writes, derived from the [`SLOT_WRITERS`]
/// calls it makes. A property named by a non-literal argument is impossible in
/// std today and would show up as a missing slot rather than a wrong one.
fn written_properties(body: &[String]) -> BTreeSet<String> {
    let mut properties = BTreeSet::new();
    for line in body {
        for (writer, position) in SLOT_WRITERS {
            let needle = format!(".{writer}(");
            let mut from = 0;
            while let Some(at) = line[from..].find(&needle) {
                let opens = from + at + needle.len();
                let arguments = arguments(&line[opens..]);
                if let Some(literal) = arguments.get(*position).and_then(|a| string_literal(a)) {
                    properties.insert(literal.to_string());
                }
                from = opens;
            }
        }
    }
    properties
}

/// `style.vl`'s `family_longhands` table, read out of the source: the properties
/// each CSS shorthand covers. The one computed arm (`border`) is rebuilt from
/// the edge and part lists `border_longhands` loops over, so a new edge or part
/// there flows through rather than needing a second copy here.
fn shorthand_longhands(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    let block = top_level_block(source, |line| line.starts_with("fun family_longhands("));
    let mut table: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in &block {
        let Some((left, right)) = line.split_once("=>") else {
            continue;
        };
        let Some(property) = string_literal(left.trim()) else {
            continue;
        };
        let right = right.trim().trim_end_matches(',');
        let longhands = match string_literal(right) {
            Some(literal) => literal
                .split(';')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect(),
            None => {
                assert_eq!(
                    right, "border_longhands()",
                    "{STYLE_SOURCE}: unrecognized family_longhands arm {right:?}"
                );
                border_longhands(source)
            }
        };
        table.insert(property.to_string(), longhands);
    }
    assert!(
        table.len() >= 6,
        "suspiciously few family_longhands arms parsed: {table:?}"
    );
    table
}

/// `border`'s longhands, rebuilt from `border_longhands`'s own seed literal and
/// its two loop lists.
fn border_longhands(source: &str) -> BTreeSet<String> {
    let block = top_level_block(source, |line| line.starts_with("fun border_longhands("));
    let text = block.join("\n");
    let seed = text
        .split_once("mut out = \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(literal, _)| literal)
        .expect("border_longhands seeds itself with a string literal");
    let mut lists = text.split("for ").skip(1).filter_map(|clause| {
        let (_, rest) = clause.split_once(" in [")?;
        let (list, _) = rest.split_once(']')?;
        Some(
            list.split(',')
                .filter_map(|item| string_literal(item.trim()))
                .map(str::to_string)
                .collect::<Vec<String>>(),
        )
    });
    let edges = lists.next().expect("border_longhands loops over the edges");
    let parts = lists.next().expect("border_longhands loops over the parts");
    assert_eq!(edges.len(), 4, "expected four border edges, got {edges:?}");
    assert_eq!(parts.len(), 3, "expected three border parts, got {parts:?}");
    let mut longhands: BTreeSet<String> = seed
        .split(';')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    for edge in &edges {
        longhands.insert(format!("border-{edge}"));
        for part in &parts {
            longhands.insert(format!("border-{edge}-{part}"));
        }
    }
    longhands
}

/// Whether two methods' slots are ENTANGLED: they write a property in common, or
/// one writes a shorthand that COVERS a property the other writes. Entangled
/// methods resolve by authoring order, so the sort must never separate them.
fn entangled(
    left: &BTreeSet<String>,
    right: &BTreeSet<String>,
    longhands: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if left.intersection(right).next().is_some() {
        return true;
    }
    let covers = |shorthands: &BTreeSet<String>, other: &BTreeSet<String>| {
        shorthands.iter().any(|property| {
            longhands
                .get(property)
                .is_some_and(|covered| covered.intersection(other).next().is_some())
        })
    };
    covers(left, right) || covers(right, left)
}

// --- 1. Completeness ---------------------------------------------------------

#[test]
fn every_style_method_is_claimed_by_the_canonical_order_table() {
    let source = style_source();
    let mut unclaimed = Vec::new();
    for (name, _) in declared_methods(&source) {
        let claimed = STYLE_PROPERTY_METHODS.iter().any(|row| row.name == name)
            || STYLE_CONDITION_METHODS
                .iter()
                .any(|(condition, _)| *condition == name)
            || STYLE_BARRIER_METHODS.contains(&name.as_str());
        if !claimed {
            unclaimed.push(name);
        }
    }
    assert!(
        unclaimed.is_empty(),
        "{STYLE_SOURCE} grew {} `Style` method(s) the canonical-order table does not know: {unclaimed:?}\n\
         Place each one in `formatter.rs`: a property method joins STYLE_PROPERTY_METHODS at its \
         Tailwind category (with the slots it writes and the family it shares), a condition joins \
         STYLE_CONDITION_METHODS on its axis, and a method whose slot is an argument joins \
         STYLE_BARRIER_METHODS. Until then `vilan fmt` treats it as a barrier and the chains \
         around it quietly stop sorting.",
        unclaimed.len()
    );
}

#[test]
fn every_table_row_names_a_method_that_exists() {
    let source = style_source();
    let declared: BTreeSet<String> = declared_methods(&source)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let mut stale = Vec::new();
    for name in STYLE_PROPERTY_METHODS
        .iter()
        .map(|row| row.name)
        .chain(STYLE_CONDITION_METHODS.iter().map(|(name, _)| *name))
        .chain(STYLE_BARRIER_METHODS.iter().copied())
    {
        if !declared.contains(name) {
            stale.push(name);
        }
    }
    assert!(
        stale.is_empty(),
        "the canonical-order table names {} method(s) {STYLE_SOURCE} no longer declares: {stale:?}",
        stale.len()
    );
}

// --- 2. Slots ----------------------------------------------------------------

#[test]
fn every_property_rows_slots_match_the_method_body() {
    let source = style_source();
    let bodies: BTreeMap<String, Vec<String>> = declared_methods(&source).into_iter().collect();
    let mut wrong = Vec::new();
    for row in STYLE_PROPERTY_METHODS {
        let body = bodies
            .get(row.name)
            .unwrap_or_else(|| panic!("{} is gated as declared by the pin above", row.name));
        let derived = written_properties(body);
        let claimed: BTreeSet<String> = row.properties.iter().map(|p| p.to_string()).collect();
        if derived != claimed {
            wrong.push(format!(
                "{}: table says {claimed:?}, {STYLE_SOURCE} writes {derived:?}",
                row.name
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the canonical-order table's slots drifted from {STYLE_SOURCE} on {} method(s):\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

// --- 3. Families -------------------------------------------------------------

#[test]
fn entangled_methods_share_a_family() {
    let source = style_source();
    let longhands = shorthand_longhands(&source);
    let slots: Vec<BTreeSet<String>> = STYLE_PROPERTY_METHODS
        .iter()
        .map(|row| row.properties.iter().map(|p| p.to_string()).collect())
        .collect();
    let mut wrong = Vec::new();
    for (left, left_row) in STYLE_PROPERTY_METHODS.iter().enumerate() {
        for (right, right_row) in STYLE_PROPERTY_METHODS.iter().enumerate().skip(left + 1) {
            if entangled(&slots[left], &slots[right], &longhands)
                && left_row.family != right_row.family
            {
                wrong.push(format!(
                    "{} ({}) and {} ({}) write entangled slots but are in different families — \
                     the sort could separate them and change the rendered style",
                    left_row.name, left_row.family, right_row.name, right_row.family
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_family_is_connected_by_entanglement() {
    // The converse: a family that groups slots which are NOT entangled would be
    // safe but useless — those methods would stop sorting past each other for no
    // reason. So each family must be one connected component.
    let source = style_source();
    let longhands = shorthand_longhands(&source);
    let mut families: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (at, row) in STYLE_PROPERTY_METHODS.iter().enumerate() {
        families.entry(row.family).or_default().push(at);
    }
    let slots: Vec<BTreeSet<String>> = STYLE_PROPERTY_METHODS
        .iter()
        .map(|row| row.properties.iter().map(|p| p.to_string()).collect())
        .collect();
    for (family, members) in families {
        let mut reached = vec![members[0]];
        let mut frontier = vec![members[0]];
        while let Some(from) = frontier.pop() {
            for member in &members {
                if !reached.contains(member) && entangled(&slots[from], &slots[*member], &longhands)
                {
                    reached.push(*member);
                    frontier.push(*member);
                }
            }
        }
        assert_eq!(
            reached.len(),
            members.len(),
            "family {family:?} is not connected by slot entanglement: {:?} are grouped for no \
             reason the slots justify",
            members
                .iter()
                .map(|at| STYLE_PROPERTY_METHODS[*at].name)
                .collect::<Vec<_>>()
        );
    }
}

// --- 4. Condition axes -------------------------------------------------------

#[test]
fn a_delegating_condition_is_recorded_on_the_axis_it_delegates_to() {
    let source = style_source();
    let axes: BTreeMap<&str, ConditionAxis> = STYLE_CONDITION_METHODS.iter().copied().collect();
    let mut wrong = Vec::new();
    for (name, body) in declared_methods(&source) {
        let Some(axis) = axes.get(name.as_str()) else {
            continue;
        };
        // `hover` is `self.pseudo("hover", inner)`; `md` is
        // `self.media("48rem", inner)`. A condition whose WHOLE body is one such
        // call must carry the axis of what it delegates to. The four primitives
        // have real bodies (and mention each other — `pseudo("dark", ..)`
        // forwards to `dark`), so the single-statement test is what separates a
        // delegation from a special case; their own axes are pinned by the order
        // pins in `formatter.rs`.
        let statements: Vec<&String> = body
            .iter()
            .filter(|line| {
                let trimmed = line.trim_start_matches('\t');
                line.len() - trimmed.len() == 2 && !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .collect();
        let [only] = statements[..] else {
            continue;
        };
        for (delegate, expected) in [
            ("pseudo", ConditionAxis::Pseudo),
            ("media", ConditionAxis::Media),
            ("dark", ConditionAxis::Dark),
            ("attribute", ConditionAxis::Attribute),
        ] {
            if name != delegate
                && only.trim_start().starts_with(&format!("self.{delegate}("))
                && *axis != expected
            {
                wrong.push(format!(
                    "{name} delegates to {delegate} but the table records it on {axis:?}"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
