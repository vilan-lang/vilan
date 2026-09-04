//! The handwritten frontend's expression/type regression corpus (H6 S5,
//! `proposal/frontend.md`).
//!
//! Through the arc, `parse_expr_differential.rs` compared the handwritten parser
//! against the chumsky ORACLE over these fixtures
//! (`parse_expr_regression/fixtures.rs`),
//! wrapping each in `let __probe = <expr>;` / `if <cond> { }` and `Debug`-comparing
//! the extracted subtree — the fine-grained precedence/grouping coverage the
//! whole-file corpus sweep (`parse_differential.rs`) cannot isolate, since no corpus
//! file is item-free. At the S5 cutover the oracle is deleted; this target keeps the
//! value ORACLE-FREE:
//!
//!   1. **Accept sweep** — every expression / condition / whole-file fixture parses
//!      cleanly (a tree, zero diagnostics, no panic).
//!   2. **Decliner sweep** — every fixture the grammar rejects still declines
//!      (non-empty diagnostics), so the parser never grows MORE permissive.
//!   3. **Precedence snapshots** — a curated precedence/grouping-critical subset is
//!      pinned to its exact `Debug`-tree (span-inclusive), the regression guard for
//!      the operator tower, `?.`-grouping, split-shift-vs-comparison, the `is` tier,
//!      unary stacking, and the H.1 struct-literal-free condition heads.

use vilan_core::node::{Node, NodeIfBranch, NodeList};
use vilan_core::parsing;
use vilan_core::span::Spanned;

// ---------------------------------------------------------------------------
// Parse + subtree extraction (oracle-free)
// ---------------------------------------------------------------------------

/// The handwritten parse, `Some(tree)` only when the source is perfectly clean
/// (a tree AND zero diagnostics) — the clean-or-decline contract.
fn parse_clean(source: &str) -> Option<Spanned<NodeList<'_>>> {
    let (tree, errors) = parsing::parse(source);
    if errors.is_empty() { tree } else { None }
}

/// The `Debug` of the `let __probe = EXPR;` initializer, or `None` if the wrapper
/// did not parse to a single clean `Let` with a value.
fn expression_subtree(fixture: &str) -> Option<String> {
    let wrapped = format!("let __probe = {fixture};");
    let tree = parse_clean(&wrapped)?;
    let (first, _) = tree.0.first()?;
    if let Node::Let(_, _, Some(value), _) = first {
        Some(format!("{value:?}"))
    } else {
        None
    }
}

/// The `Debug` of the `if EXPR { }` condition, or `None` if the wrapper did not
/// parse to a single clean `If`.
fn condition_subtree(fixture: &str) -> Option<String> {
    let wrapped = format!("if {fixture} {{ }}");
    let tree = parse_clean(&wrapped)?;
    let (first, _) = tree.0.first()?;
    if let Node::If(NodeIfBranch::If(if_)) = first {
        Some(format!("{:?}", if_.condition))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 1. Accept sweep — every fixture parses cleanly, no panic
// ---------------------------------------------------------------------------

#[test]
fn every_expression_fixture_parses_cleanly() {
    let mut rejected: Vec<String> = Vec::new();
    for fixture in EXPRESSION_FIXTURES {
        if expression_subtree(fixture).is_none() {
            let (_, errors) = parsing::parse(&format!("let __probe = {fixture};"));
            rejected.push(format!(
                "`{fixture}`: {} diagnostic(s){}",
                errors.len(),
                errors
                    .first()
                    .map(|error| format!(" — {}", parsing::render(error)))
                    .unwrap_or_default()
            ));
        }
    }
    assert!(
        rejected.is_empty(),
        "the parser rejected {} expression fixture(s) it must accept:\n{}",
        rejected.len(),
        rejected.join("\n")
    );
}

#[test]
fn every_condition_fixture_parses_cleanly() {
    let rejected: Vec<&str> = CONDITION_FIXTURES
        .iter()
        .filter(|fixture| condition_subtree(fixture).is_none())
        .copied()
        .collect();
    assert!(
        rejected.is_empty(),
        "the parser rejected {} condition fixture(s) it must accept: {:?}",
        rejected.len(),
        rejected
    );
}

#[test]
fn every_whole_file_fixture_parses_cleanly() {
    let rejected: Vec<&str> = WHOLE_FILE_FIXTURES
        .iter()
        .filter(|fixture| parse_clean(fixture).is_none())
        .copied()
        .collect();
    assert!(
        rejected.is_empty(),
        "the parser rejected {} whole-file fixture(s) it must accept: {:?}",
        rejected.len(),
        rejected
    );
}

// ---------------------------------------------------------------------------
// 2. Decliner sweep — every rejected shape still declines
// ---------------------------------------------------------------------------

#[test]
fn every_decliner_fixture_is_rejected() {
    let mut wrongly_accepted: Vec<&str> = Vec::new();
    for fixture in DECLINER_FIXTURES {
        // A decliner is rejected iff its `let __probe = <fixture>;` wrapper produces
        // at least one diagnostic (the parser is not MORE permissive than the
        // grammar). The handwritten frontend always returns a (recovered) tree, so
        // the contract lives in the ERROR LIST, not a missing tree.
        let (_, errors) = parsing::parse(&format!("let __probe = {fixture};"));
        if errors.is_empty() {
            wrongly_accepted.push(fixture);
        }
    }
    assert!(
        wrongly_accepted.is_empty(),
        "the parser ACCEPTED {} fixture(s) the grammar rejects: {:?}",
        wrongly_accepted.len(),
        wrongly_accepted
    );
}

// ---------------------------------------------------------------------------
// 3. Precedence / grouping snapshots — the shapes a whole-file sweep cannot catch
// ---------------------------------------------------------------------------

/// Assert a fixture's extracted expression subtree `Debug` exactly (span-inclusive).
#[track_caller]
fn snapshot(fixture: &str, expected: &str) {
    assert_eq!(
        expression_subtree(fixture).as_deref(),
        Some(expected),
        "precedence snapshot changed for `{fixture}`"
    );
}

/// Same, for a struct-literal-free condition head (§H.1).
#[track_caller]
fn snapshot_condition(fixture: &str, expected: &str) {
    assert_eq!(
        condition_subtree(fixture).as_deref(),
        Some(expected),
        "condition snapshot changed for `{fixture}`"
    );
}

#[test]
fn the_operator_tower_groups_as_pinned() {
    // Arithmetic precedence + associativity.
    snapshot("a + b * c", SNAP_ADD_MUL);
    snapshot("a * b + c", SNAP_MUL_ADD);
    snapshot("a - b - c", SNAP_SUB_SUB);
    snapshot("a * b % c", SNAP_MUL_REM);
    // The bit tiers: `&` tighter than `^` tighter than `|`.
    snapshot("a & b ^ c | d", SNAP_BIT_TOWER);
    snapshot("a | b & c", SNAP_OR_AND);
    // Shifts vs arithmetic vs comparison (the split-`<<` / split-`>>` seam).
    snapshot("a << 2 + 1", SNAP_SHL_ADD);
    snapshot("a + 1 << 2", SNAP_ADD_SHL);
    snapshot("a << b < c", SNAP_SHL_LT);
    // Comparison, then logical `&&` then `||`.
    snapshot("a < b == c", SNAP_LT_EQ);
    snapshot("a == b && c", SNAP_EQ_AND);
    snapshot("a && b || c", SNAP_AND_OR);
    snapshot("a || b && c", SNAP_OR_LOGICAL);
}

#[test]
fn unary_and_prefix_stacking_groups_as_pinned() {
    snapshot("!a == b", SNAP_NOT_EQ);
    snapshot("-a + b", SNAP_NEG_ADD);
    snapshot("a + -b", SNAP_ADD_NEG);
    snapshot("- -x", SNAP_NEG_NEG);
    snapshot("!!x", SNAP_NOT_NOT);
    snapshot("&mut a.b", SNAP_REFMUT_MEMBER);
    snapshot("*&x", SNAP_DEREF_REF);
}

#[test]
fn the_postfix_chain_and_lift_grouping_are_pinned() {
    snapshot("a.b.c.d", SNAP_MEMBER_CHAIN);
    snapshot("a.method().field", SNAP_CALL_MEMBER);
    snapshot("a[i][j]", SNAP_INDEX_INDEX);
    snapshot("a.b[0].c", SNAP_MEMBER_INDEX);
    snapshot("f(g(h(x)))", SNAP_NESTED_CALLS);
    snapshot("a.read()(x)", SNAP_CALL_OF_CALL);
    // `?.` grouping and lift regions.
    snapshot("a?.b?.c", SNAP_TRY_CHAIN);
    snapshot("a.b?.c.d", SNAP_MEMBER_TRY);
    snapshot("(a? + b?)", SNAP_LIFT_ADD);
    snapshot("(x?.y + z)", SNAP_LIFT_MEMBER);
}

#[test]
fn element_atoms_are_pinned() {
    // The element form's parse shape, span-inclusive (element-syntax S2): one
    // attribute head item, one string child. The desugar-critical structure —
    // head item kinds, child list, tag span — is all visible here, and so are
    // the four angle-bracket spans the editor paints from (E115): `<` at
    // 14..15, the head's `>` at 27..28, `</` at 32..34 and its `>` at 35..36.
    snapshot("<p class(\"x\")>\"hi\"</p>", SNAP_ELEMENT);
}

const SNAP_ELEMENT: &str = "(Element(ElementBody { tag: 15..16, head: [Attribute(17..22, Some((String(\"x\"), 23..26)))], children: [Bare((String(\"hi\"), 28..32))], self_closing: false, close_tag: Some(34..35), punctuation: [14..15, 27..28, 32..34, 35..36] }), 14..36)";

#[test]
fn the_is_tier_and_condition_heads_are_pinned() {
    snapshot("a is None && b", SNAP_IS_AND);
    snapshot("x is Some(let y)", SNAP_IS_BIND);
    snapshot("x is Some(let (y, z))", SNAP_IS_BIND_TUPLE);
    // §H.1 struct-literal-free condition heads.
    snapshot_condition("flag & mask == 0", SNAP_COND_BIT_EQ);
    snapshot_condition("a << 2 > b", SNAP_COND_SHL_GT);
    snapshot_condition("a is None || b is None", SNAP_COND_IS_OR);
    snapshot_condition("(Foo { x = 1 })", SNAP_COND_PAREN_STRUCT);
}

// Precedence/grouping snapshots, captured from the handwritten parser
// (span-inclusive; regenerate with the `generate_snapshot_consts` history if a
// deliberate grammar change moves them).
const SNAP_ADD_MUL: &str = "(Binary(Add, (Accessor(\"a\"), 14..15), (Binary(Mul, (Accessor(\"b\"), 18..19), (Accessor(\"c\"), 22..23)), 18..23)), 14..23)";
const SNAP_MUL_ADD: &str = "(Binary(Add, (Binary(Mul, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 18..19)), 14..19), (Accessor(\"c\"), 22..23)), 14..23)";
const SNAP_SUB_SUB: &str = "(Binary(Sub, (Binary(Sub, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 18..19)), 14..19), (Accessor(\"c\"), 22..23)), 14..23)";
const SNAP_MUL_REM: &str = "(Binary(Rem, (Binary(Mul, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 18..19)), 14..19), (Accessor(\"c\"), 22..23)), 14..23)";
const SNAP_BIT_TOWER: &str = "(Binary(BitOr, (Binary(BitXor, (Binary(BitAnd, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 18..19)), 14..19), (Accessor(\"c\"), 22..23)), 14..23), (Accessor(\"d\"), 26..27)), 14..27)";
const SNAP_OR_AND: &str = "(Binary(BitOr, (Accessor(\"a\"), 14..15), (Binary(BitAnd, (Accessor(\"b\"), 18..19), (Accessor(\"c\"), 22..23)), 18..23)), 14..23)";
const SNAP_SHL_ADD: &str = "(Binary(Shl, (Accessor(\"a\"), 14..15), (Binary(Add, (Number(\"2\", None, None), 19..20), (Number(\"1\", None, None), 23..24)), 19..24)), 14..24)";
const SNAP_ADD_SHL: &str = "(Binary(Shl, (Binary(Add, (Accessor(\"a\"), 14..15), (Number(\"1\", None, None), 18..19)), 14..19), (Number(\"2\", None, None), 23..24)), 14..24)";
const SNAP_SHL_LT: &str = "(Binary(Lt, (Binary(Shl, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 19..20)), 14..20), (Accessor(\"c\"), 23..24)), 14..24)";
const SNAP_LT_EQ: &str = "(Binary(Eq, (Binary(Lt, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 18..19)), 14..19), (Accessor(\"c\"), 23..24)), 14..24)";
const SNAP_EQ_AND: &str = "(Binary(And, (Binary(Eq, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 19..20)), 14..20), (Accessor(\"c\"), 24..25)), 14..25)";
const SNAP_AND_OR: &str = "(Binary(Or, (Binary(And, (Accessor(\"a\"), 14..15), (Accessor(\"b\"), 19..20)), 14..20), (Accessor(\"c\"), 24..25)), 14..25)";
const SNAP_OR_LOGICAL: &str = "(Binary(Or, (Accessor(\"a\"), 14..15), (Binary(And, (Accessor(\"b\"), 19..20), (Accessor(\"c\"), 24..25)), 19..25)), 14..25)";
const SNAP_NOT_EQ: &str = "(Binary(Eq, (Unary('!', (Accessor(\"a\"), 15..16)), 14..16), (Accessor(\"b\"), 20..21)), 14..21)";
const SNAP_NEG_ADD: &str = "(Binary(Add, (Unary('-', (Accessor(\"a\"), 15..16)), 14..16), (Accessor(\"b\"), 19..20)), 14..20)";
const SNAP_ADD_NEG: &str = "(Binary(Add, (Accessor(\"a\"), 14..15), (Unary('-', (Accessor(\"b\"), 19..20)), 18..20)), 14..20)";
const SNAP_NEG_NEG: &str = "(Unary('-', (Unary('-', (Accessor(\"x\"), 17..18)), 16..18)), 14..18)";
const SNAP_NOT_NOT: &str = "(Unary('!', (Unary('!', (Accessor(\"x\"), 16..17)), 15..17)), 14..17)";
const SNAP_REFMUT_MEMBER: &str = "(Reference(true, (MemberAccessor((Accessor(\"a\"), 19..20), (Accessor(\"b\"), 21..22)), 19..22)), 14..22)";
const SNAP_DEREF_REF: &str =
    "(Dereference((Reference(false, (Accessor(\"x\"), 16..17)), 15..17)), 14..17)";
const SNAP_MEMBER_CHAIN: &str = "(MemberAccessor((MemberAccessor((MemberAccessor((Accessor(\"a\"), 14..15), (Accessor(\"b\"), 16..17)), 14..17), (Accessor(\"c\"), 18..19)), 14..19), (Accessor(\"d\"), 20..21)), 14..21)";
const SNAP_CALL_MEMBER: &str = "(MemberAccessor((MemberAccessor((Accessor(\"a\"), 14..15), (Call((Accessor(\"method\"), 16..22), None, ([], 22..24)), 16..24)), 14..24), (Accessor(\"field\"), 25..30)), 14..30)";
const SNAP_INDEX_INDEX: &str = "(Index((Index((Accessor(\"a\"), 14..15), (Accessor(\"i\"), 16..17)), 14..18), (Accessor(\"j\"), 19..20)), 14..21)";
const SNAP_MEMBER_INDEX: &str = "(MemberAccessor((Index((MemberAccessor((Accessor(\"a\"), 14..15), (Accessor(\"b\"), 16..17)), 14..17), (Number(\"0\", None, None), 18..19)), 14..20), (Accessor(\"c\"), 21..22)), 14..22)";
const SNAP_NESTED_CALLS: &str = "(Call((Accessor(\"f\"), 14..15), None, ([(Call((Accessor(\"g\"), 16..17), None, ([(Call((Accessor(\"h\"), 18..19), None, ([(Accessor(\"x\"), 20..21)], 19..22)), 18..22)], 17..23)), 16..23)], 15..24)), 14..24)";
const SNAP_CALL_OF_CALL: &str = "(Call((MemberAccessor((Accessor(\"a\"), 14..15), (Call((Accessor(\"read\"), 16..20), None, ([], 20..22)), 16..22)), 14..22), None, ([(Accessor(\"x\"), 23..24)], 22..25)), 14..25)";
const SNAP_TRY_CHAIN: &str = "(Lift((Lift((Accessor(\"a\"), 14..15), (MemberAccessor((LiftBinder, 17..18), (Accessor(\"b\"), 17..18)), 17..18)), 14..18), (MemberAccessor((LiftBinder, 20..21), (Accessor(\"c\"), 20..21)), 20..21)), 14..21)";
const SNAP_MEMBER_TRY: &str = "(Lift((MemberAccessor((Accessor(\"a\"), 14..15), (Accessor(\"b\"), 16..17)), 14..17), (MemberAccessor((MemberAccessor((LiftBinder, 19..20), (Accessor(\"c\"), 19..20)), 19..20), (Accessor(\"d\"), 21..22)), 19..22)), 14..22)";
const SNAP_LIFT_ADD: &str = "(LiftGroup((Binary(Add, (Lifted((Accessor(\"a\"), 15..16)), 15..17), (Lifted((Accessor(\"b\"), 20..21)), 20..22)), 15..22)), 14..23)";
const SNAP_LIFT_MEMBER: &str = "(Binary(Add, (Lift((Accessor(\"x\"), 15..16), (MemberAccessor((LiftBinder, 18..19), (Accessor(\"y\"), 18..19)), 18..19)), 15..19), (Accessor(\"z\"), 22..23)), 15..23)";
const SNAP_IS_AND: &str = "(Binary(And, (Is((Accessor(\"a\"), 14..15), (Variant([\"None\"], None), 19..23)), 14..23), (Accessor(\"b\"), 27..28)), 14..28)";
// E111: `Binding`'s third field is the NAME's own span, recorded by the parser
// where it is unambiguous. Read it against the pattern span beside it: the
// pattern is `let y` at 24..29 and the name is `y` at 28..29, so the four
// characters of the keyword are accounted for HERE and not by arithmetic in a
// consumer. The tuple fixture below is the case that arithmetic got wrong.
const SNAP_IS_BIND: &str = "(Is((Accessor(\"x\"), 14..15), (Variant([\"Some\"], Some([(Binding(\"y\", false, 28..29), 24..29)])), 19..30)), 14..30)";
// A BINDER tuple payload: the tuple pattern spans `let (y, z)` (24..34, keyword
// included) while each element carries its bare identifier — `y` at 29..30, `z`
// at 32..33. Adding the keyword's four characters to an element start, which is
// what the analyzer used to do, lands `y` on `, z` and `z` past the parens (E111).
const SNAP_IS_BIND_TUPLE: &str = "(Is((Accessor(\"x\"), 14..15), (Variant([\"Some\"], Some([(Tuple([(Binding(\"y\", false, 29..30), 29..30), (Binding(\"z\", false, 32..33), 32..33)]), 24..34)])), 19..35)), 14..35)";
const SNAP_COND_BIT_EQ: &str = "(Binary(Eq, (Binary(BitAnd, (Accessor(\"flag\"), 3..7), (Accessor(\"mask\"), 10..14)), 3..14), (Number(\"0\", None, None), 18..19)), 3..19)";
const SNAP_COND_SHL_GT: &str = "(Binary(Gt, (Binary(Shl, (Accessor(\"a\"), 3..4), (Number(\"2\", None, None), 8..9)), 3..9), (Accessor(\"b\"), 12..13)), 3..13)";
const SNAP_COND_IS_OR: &str = "(Binary(Or, (Is((Accessor(\"a\"), 3..4), (Variant([\"None\"], None), 8..12)), 3..12), (Is((Accessor(\"b\"), 16..17), (Variant([\"None\"], None), 21..25)), 16..25)), 3..25)";
const SNAP_COND_PAREN_STRUCT: &str = "(StructInitializer([], (\"Foo\", 4..7), None, ([((\"x\", Some((Number(\"1\", None, None), 14..15))), 10..15)], 8..17)), 4..17)";

// ---------------------------------------------------------------------------
// 4. Located decliner messages — the shapes whose WORDING is the contract
// ---------------------------------------------------------------------------

/// The rendered diagnostics of `source`, each paired with the source text its
/// span covers — so a pin holds the anchor as well as the wording.
fn diagnostics_at(source: &str) -> Vec<(String, String)> {
    parsing::parse(source)
        .1
        .iter()
        .map(|error| {
            let range = error.span.into_range();
            (
                parsing::render(error),
                source[range.start..range.end].to_string(),
            )
        })
        .collect()
}

/// E135. `a::` is in the decliner corpus above, which only asserts that SOMETHING
/// was reported; the message is the point. An unfinished path used to roll its
/// `::` back, leaving the enclosing statement to complain about a missing `;` at
/// the operator — a true statement about a program nobody wrote, and one that
/// never named the thing that is actually absent.
#[test]
fn an_unfinished_path_names_the_missing_name() {
    assert_eq!(
        diagnostics_at("let __probe = a::;"),
        vec![(
            "found ';' expected a name after `::`".to_string(),
            ";".to_string()
        )],
    );
}

/// E135, the two shapes the owner hit while migrating kolt. Each is ONE parse
/// error, anchored on the token standing where the name should be.
///
/// The first is the face that used to report `expected ';' to end this
/// statement` on the `::` — the element on the next line read as a comparison
/// once the `::` rolled back, and the whole statement degenerated. The second is
/// the same mistake terminated on its own line.
#[test]
fn an_unfinished_path_before_an_element_reports_once_at_the_element() {
    let source = "fun panel(): View {\n\tlet x = style::\n\t<div class(\"panel\")></div>\n}\n";
    assert_eq!(
        diagnostics_at(source),
        vec![(
            "found '<' expected a name after `::`".to_string(),
            "<".to_string()
        )],
    );
}

#[test]
fn an_unfinished_path_reports_once_when_the_statement_is_terminated() {
    let source = "fun panel(): View {\n\tlet x = style::;\n\t<div></div>\n}\n";
    assert_eq!(
        diagnostics_at(source),
        vec![(
            "found ';' expected a name after `::`".to_string(),
            ";".to_string()
        )],
    );
}

// The fixture arrays live in a SUBDIRECTORY, not beside this file: cargo makes a
// test target out of every `tests/*.rs`, so a fixtures file at the top level was
// also compiled as a target of its own — a binary with no tests in it, whose only
// output was four "never used" warnings against the arrays this file consumes.
include!("parse_expr_regression/fixtures.rs");
