//! Per-site parser/lexer recovery pins (H6 S0, `proposal/frontend.md` §3).
//!
//! The handwritten frontend that replaces chumsky (H6) must reproduce the
//! CURRENT parser's recovery behavior byte-for-byte. §0 of the proposal notes
//! that only the trailing-`.` member case (`member_completion_on_incomplete_receiver`
//! in vilan-lsp) is pinned as an observable today; the ten `nested_delimiters`
//! sites, the `?.` sibling, the misplaced-`resource` steer, and the lexer's
//! skip-then-retry are exercised only indirectly. These pins make each one an
//! explicit contract, asserting — against the handwritten `parsing::parse` (which
//! S4 gave recovery; through the arc these also ran against the chumsky oracle,
//! now deleted — see `frontends`) — that a garbled input at the site:
//!   (a) does NOT hard-fail (a partial tree comes back),
//!   (b) recovers to the documented placeholder (empty vec / `None` / `Node::Error`
//!       / empty block), and
//!   (c) reports a diagnostic (the error is not swallowed) — for the member cases
//!       the diagnostic surfaces during analysis, so those are pinned at both the
//!       parse level (tree shape) and the analyze level (see the analyze module).
//!
//! Diagnostic *counts and wording* are deliberately NOT over-pinned: proposal §6(a)
//! allows parse errors to improve at cutover. The pins assert "at least one error",
//! never an exact count. Recovered-tree *spans* ARE part of the contract (the S0
//! differential harness is span-inclusive) so the shape substrings carry them.
//!
//! Every recovered shape below was captured from the current binary, not asserted
//! from reading (H6 S0 is probe-first).

use vilan_core::parsing;

/// The handwritten frontend (H6 S4). Through the arc these pins ran against BOTH
/// this and the chumsky oracle (proven byte-identical); at the S5 cutover the
/// oracle is deleted and the pins hold the recovered SHAPES on the handwritten
/// frontend alone. Returns the recovered tree's `Debug` (if any) and the diagnostic
/// count.
fn handwritten_recovered(source: &str) -> (Option<String>, usize) {
    let (tree, errors) = parsing::parse(source);
    (tree.map(|tree| format!("{tree:?}")), errors.len())
}

/// The frontends every parse-level pin runs against. Post-cutover this is the
/// handwritten frontend alone (the `for_each_frontend` loop is retained so the
/// pins read unchanged).
type Frontend = fn(&str) -> (Option<String>, usize);
fn frontends() -> [(&'static str, Frontend); 1] {
    [("handwritten", handwritten_recovered)]
}

/// Run `check` against every frontend's recovery of `source`, asserting first that
/// recovery did NOT hard-fail (a partial tree came back — contract (a)). `check`
/// receives the frontend's name (for the failure message), the tree's `Debug`, and
/// the diagnostic count.
#[track_caller]
fn for_each_frontend(source: &str, check: impl Fn(&str, &str, usize)) {
    for (name, frontend) in frontends() {
        let (tree, errors) = frontend(source);
        let tree = tree.unwrap_or_else(|| {
            panic!("[{name}] recovery must yield a partial tree, not hard-fail (a), for {source:?}")
        });
        check(name, &tree, errors);
    }
}

// --- The ten `nested_delimiters` sites (parser.rs, verified 2026-07-21) --------

#[test]
fn recovers_garbled_generic_parameters() {
    // parser.rs ~248: a garbled `<...>` generic-PARAMETER list (on a declaration)
    // recovers via `nested_delimiters(<, >, .., |span| (Vec::new(), span))` to an
    // EMPTY parameter vec.
    for_each_frontend("fun f<1 2 3>() {}\n", |name, tree, errors| {
        assert!(
            errors > 0,
            "[{name}] garbled generic parameters must report (c): {tree}"
        );
        assert!(
            tree.contains("generic_parameters: Some(([]"),
            "[{name}] recovered to an empty generic-parameter vec (b); got: {tree}"
        );
    });
}

#[test]
fn recovers_garbled_generic_arguments() {
    // parser.rs ~269: a garbled `<...>` generic-ARGUMENT list (in a type position)
    // recovers to an EMPTY argument vec — here on the type `List<..>`.
    for_each_frontend("fun f(x: List<1 2 3>) {}\n", |name, tree, errors| {
        assert!(
            errors > 0,
            "[{name}] garbled generic arguments must report (c): {tree}"
        );
        assert!(
            tree.contains("AccessorWithGenerics(\"List\", ([]"),
            "[{name}] recovered to an empty generic-argument vec (b); got: {tree}"
        );
    });
}

#[test]
fn recovers_a_garbled_element_to_an_error_atom() {
    // A committed element whose parse fails recovers the balanced `<…>` head
    // region to an `Error` atom (element-syntax S2): a partial tree (a), a
    // documented placeholder (b), at least one diagnostic (c).
    for_each_frontend(
        "fun main() { let p = <div 1 2>; }\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] a garbled element must report (c): {tree}"
            );
            assert!(
                tree.contains("Error"),
                "[{name}] recovered to an Error placeholder (b); got: {tree}"
            );
        },
    );
}

#[test]
fn recovers_an_unfinished_chain_link_keeping_the_element() {
    // E67 (editing-dx.md §18): a head item whose `.` has no method name yet is
    // a COMMITTED production failing — the dot already chose the chain form —
    // so it recovers in place: the item is dropped and reported, and the
    // element survives. Declining instead let the element recovery above
    // flatten the tag to an `Error` atom and, with the tag nested, take the
    // whole statement with it, which is what left the language server with
    // nothing to answer from inside a tag under construction.
    for_each_frontend(
        "fun main() { let p = <div><span .></span></div>; }\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] an unfinished chain link must report (c): {tree}"
            );
            assert_eq!(
                tree.matches("ElementBody").count(),
                2,
                "[{name}] both elements survive the recovery (b); got: {tree}"
            );
            assert!(
                !tree.contains("Error"),
                "[{name}] no error atom is left behind (b); got: {tree}"
            );
        },
    );
}

#[test]
fn recovers_garbled_struct_initializer_fields() {
    // parser.rs ~299: a garbled `Name { .. }` struct-initializer field list
    // recovers via `|span| (None, span)` (then mapped to an empty vec) to EMPTY
    // fields.
    for_each_frontend(
        "fun main() { let p = Point { 1 2 3 }; }\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] garbled struct-init fields must report (c): {tree}"
            );
            assert!(
                tree.contains("StructInitializer(\"Point\", None, ([]"),
                "[{name}] recovered to empty struct-initializer fields (b); got: {tree}"
            );
        },
    );
}

#[test]
fn recovers_garbled_parenthesized_expression() {
    // parser.rs ~432: a garbled `( .. )` expression group recovers via
    // `|span| (Node::Error, span)` to a `Node::Error` in expression position.
    // (The shape is shared with the list site below; the `(` delimiter is what
    // routes recovery here — only the paren-recovery can fire on a paren group.)
    for_each_frontend("fun main() { let x = (1 +); }\n", |name, tree, errors| {
        assert!(
            errors > 0,
            "[{name}] garbled paren group must report (c): {tree}"
        );
        assert!(
            tree.contains("Let((\"x\", 17..18), None, Some((Error,"),
            "[{name}] garbled paren recovered to a Node::Error expression (b); got: {tree}"
        );
    });
}

#[test]
fn recovers_garbled_list_literal() {
    // parser.rs ~442: a garbled `[ .. ]` list literal recovers via
    // `|span| (Node::Error, span)` to a `Node::Error` in expression position.
    // The `[` delimiter routes recovery to the list site (not the paren site).
    for_each_frontend("fun main() { let x = [1 +]; }\n", |name, tree, errors| {
        assert!(
            errors > 0,
            "[{name}] garbled list literal must report (c): {tree}"
        );
        assert!(
            tree.contains("Let((\"x\", 17..18), None, Some((Error,"),
            "[{name}] garbled list recovered to a Node::Error expression (b); got: {tree}"
        );
    });
}

#[test]
fn recovers_garbled_block() {
    // parser.rs ~539: a garbled `{ .. }` block recovers to an EMPTY block (no
    // statements, a `Void` tail). The non-empty source with `errors > 0` proves
    // this is recovery, not a legitimately-empty `fun main() {}`.
    //
    // The SHAPE is unchanged since `editing-dx.md` S1 retired the region-skipping
    // arm this site used to take: the block's one statement is now recovered
    // individually and dropped individually, which leaves the same empty block
    // here — and, unlike region-skipping, leaves a body's OTHER statements in
    // place (`a_broken_statement_keeps_its_siblings_in_the_body`).
    for_each_frontend("fun main() { let x = 1 + ; }\n", |name, tree, errors| {
        assert!(errors > 0, "[{name}] garbled block must report (c): {tree}");
        assert!(
            tree.contains("body: Some((([], (Void,"),
            "[{name}] garbled block recovered to an empty block (b); got: {tree}"
        );
    });
}

#[test]
fn recovers_garbled_struct_body() {
    // parser.rs ~1160: a garbled `struct N { .. }` body recovers via
    // `|span| (None, span)` (mapped to an empty vec) to an EMPTY braced body.
    for_each_frontend("struct S { 1 2 3 }\n", |name, tree, errors| {
        assert!(
            errors > 0,
            "[{name}] garbled struct body must report (c): {tree}"
        );
        assert!(
            tree.contains("Struct((\"S\", 7..8), None, false, false, Some(([]"),
            "[{name}] garbled struct body recovered to empty fields (b); got: {tree}"
        );
    });
}

#[test]
fn recovers_garbled_impl_body_and_continues() {
    // parser.rs ~1210: a garbled `impl X { .. }` body recovers via
    // `|span| (Vec::new(), span)` to an EMPTY body, AND the following item still
    // parses (recovery synchronizes at the item boundary).
    for_each_frontend(
        "impl Foo { 1 2 3 }\nfun after() {}\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] garbled impl body must report (c): {tree}"
            );
            assert!(
                tree.contains("Impl((Accessor(\"Foo\"), 5..8), [], ([]"),
                "[{name}] garbled impl body recovered to an empty body (b); got: {tree}"
            );
            assert!(
                tree.contains("(\"after\""),
                "[{name}] the item after a recovered impl body must still parse; got: {tree}"
            );
        },
    );
}

#[test]
fn recovers_garbled_trait_body_and_continues() {
    // parser.rs ~1252: a garbled `trait X { .. }` body recovers via
    // `|span| (Vec::new(), span)` to an EMPTY body, and the following item parses.
    for_each_frontend(
        "trait Foo { 1 2 3 }\nfun after() {}\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] garbled trait body must report (c): {tree}"
            );
            assert!(
                tree.contains("Trait((\"Foo\", 6..9), None, [], ([]"),
                "[{name}] garbled trait body recovered to an empty body (b); got: {tree}"
            );
            assert!(
                tree.contains("(\"after\""),
                "[{name}] the item after a recovered trait body must still parse; got: {tree}"
            );
        },
    );
}

#[test]
fn recovers_garbled_module_body_and_continues() {
    // parser.rs ~1280: a garbled `mod X { .. }` body recovers via
    // `|span| (Vec::new(), span)` to an EMPTY body, and the following item parses.
    for_each_frontend(
        "mod foo { 1 2 3 }\nfun after() {}\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] garbled module body must report (c): {tree}"
            );
            assert!(
                tree.contains("Module(\"foo\", ([]"),
                "[{name}] garbled module body recovered to an empty body (b); got: {tree}"
            );
            assert!(
                tree.contains("(\"after\""),
                "[{name}] the item after a recovered module body must still parse; got: {tree}"
            );
        },
    );
}

// --- The member-recovery siblings (parse-level tree shape) ---------------------

#[test]
fn recovers_trailing_dot_member_keeping_receiver() {
    // parser.rs ~1933: a trailing `.` with no member (`p.`, mid-edit) recovers to
    // `Postfix::Member((Node::Error, dot_span))` while KEEPING the receiver — the
    // property the LSP's member completion relies on (its analyze-level diagnostic
    // is pinned in the `analyze` module below). This recovery is deliberately
    // SILENT at parse (0 parse errors), so no `errors` assertion here.
    for_each_frontend(
        "fun main() { let p = Point { x = 1 }; p. }\n",
        |name, tree, _errors| {
            assert!(
                tree.contains("MemberAccessor((Accessor(\"p\")"),
                "[{name}] the receiver `p` must survive the trailing `.` (b); got: {tree}"
            );
            assert!(
                tree.contains(", (Error,"),
                "[{name}] the missing member is a `Node::Error` placeholder (b); got: {tree}"
            );
        },
    );
}

#[test]
fn recovers_trailing_question_dot_member_keeping_receiver() {
    // parser.rs ~1958: the `?.` sibling of the trailing-`.` recovery — `p?.`
    // mid-edit recovers to `Postfix::LiftMember((Node::Error, dot_span))`, keeping
    // the receiver `p`. Also silent at parse (0 errors); the lift diagnostic is
    // pinned at the analyze level.
    for_each_frontend(
        "fun main() { let p = Point { x = 1 }; p?. }\n",
        |name, tree, _errors| {
            assert!(
                tree.contains("Lift((Accessor(\"p\")"),
                "[{name}] the receiver `p` must survive the trailing `?.` (b); got: {tree}"
            );
            assert!(
                tree.contains(", (Error,"),
                "[{name}] the missing `?.` member is a `Node::Error` placeholder (b); got: {tree}"
            );
        },
    );
}

// --- The misplaced-`resource` steer (recovery half) ----------------------------

#[test]
fn recovers_misplaced_resource_and_continues() {
    // parser.rs ~1501: `resource` before anything but `struct`/`enum` steers — it
    // emits a diagnostic and a `Node::Error` placeholder, leaving the offending
    // token unconsumed so `fun`/`impl`/`let`/`trait` parse as themselves. The
    // MESSAGE is already pinned in the `inference` suite
    // (`resource_on_a_*_is_rejected`); this pins the RECOVERY half — the steer placeholder plus the fact that the
    // steered item AND every subsequent item still parse.
    for_each_frontend(
        "resource fun foo() {}\nfun after() {}\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] the misplaced `resource` must report (c): {tree}"
            );
            assert!(
                tree.contains("(Error, 0..8)"),
                "[{name}] `resource` steered to a Node::Error placeholder (b); got: {tree}"
            );
            assert!(
                tree.contains("(\"foo\"") && tree.contains("(\"after\""),
                "[{name}] the steered `fun foo` and the following `fun after` must both parse; got: {tree}"
            );
        },
    );
}

// --- The lexer's skip-then-retry (lexer.rs ~257) -------------------------------

#[test]
fn lexer_skips_an_illegal_character_and_lexes_the_rest() {
    // lexer.rs ~257: `.recover_with(skip_then_retry_until(any().ignored(), end()))`
    // — an illegal character (here U+0007 BEL, which matches no token) is reported
    // and skipped, and the rest of the file lexes and parses normally. (The
    // handwritten lexer, S1, records the same skip; the char is mid-file, so
    // chumsky does NOT discard the stream — both frontends agree here.)
    for_each_frontend(
        "fun main() { let x = 1; \u{0007} let y = 2; }\n",
        |name, tree, errors| {
            assert!(
                errors > 0,
                "[{name}] the illegal character must report (c): {tree}"
            );
            assert!(
                tree.contains("(\"x\"") && tree.contains("(\"y\""),
                "[{name}] both statements around the illegal character must parse (b); got: {tree}"
            );
        },
    );
}

// --- Analyze-level pins for the member/resource contracts ----------------------
//
// The member recoveries are silent at parse; the diagnostic — and the proof the
// receiver still TYPES — surfaces during analysis. These pins mirror the LSP's
// `member_completion_on_incomplete_receiver` contract at the core level (per the
// H6 S0 work order), without touching vilan-lsp.
mod analyze {
    use std::path::{Path, PathBuf};
    use vilan_core::{Workspace, analyze_source};

    fn std_spec() -> vilan_core::PackageSpec {
        vilan_core::manifest::resolve_std(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
        )
    }

    /// Analyze `source` and return `(program_came_back, diagnostic_messages)`.
    #[track_caller]
    fn analyze(source: &str) -> (bool, Vec<String>) {
        let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
        let (program, errors) = analyze_source(
            leaked,
            &std_spec(),
            Path::new("."),
            Path::new("test.vl"),
            None,
            &Workspace::default(),
        );
        (
            program.is_some(),
            errors.into_iter().map(|error| error.msg).collect(),
        )
    }

    #[test]
    fn trailing_dot_member_analyzes_with_a_diagnostic() {
        // The receiver `p: Point` is typed despite the trailing `.`; the analyzer
        // reports the missing member rather than discarding the statement.
        let (program, messages) = analyze(
            "struct Point { x: i32, y: i32 }\n\
             fun main() { let p = Point { x = 1, y = 2 }; p. }\n",
        );
        assert!(program, "a recovered `p.` must still produce a Program");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("field or method name after")),
            "the missing member must be diagnosed; got: {messages:#?}"
        );
    }

    #[test]
    fn trailing_question_dot_member_types_the_receiver() {
        // The decisive proof the receiver still TYPES: the `?.` lift diagnostic
        // names the receiver's resolved type ("this is Point"), which the analyzer
        // could only know by typing `p` — exactly what completion after `p?.`
        // needs. Mirrors the LSP `member_completion_on_incomplete_receiver` pin.
        let (program, messages) = analyze(
            "struct Point { x: i32, y: i32 }\n\
             fun main() { let p = Point { x = 1, y = 2 }; p?. }\n",
        );
        assert!(program, "a recovered `p?.` must still produce a Program");
        assert!(
            messages.iter().any(|m| m.contains("this is Point")),
            "the receiver must type to Point despite the trailing `?.`; got: {messages:#?}"
        );
    }

    #[test]
    fn misplaced_resource_analyzes_the_rest_of_the_file() {
        // The recovery half at the analyze level: after the steered `resource fun`,
        // the following struct + function still analyze (the sole diagnostic is the
        // steer message, and `Point` is usable downstream).
        let (program, messages) = analyze(
            "resource fun foo() {}\n\
             struct Point { x: i32 }\n\
             fun after() { let q = Point { x = 5 }; }\n",
        );
        assert!(program, "a steered `resource` must still produce a Program");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("type-declaration modifier")),
            "the steer diagnostic must be present; got: {messages:#?}"
        );
        assert!(
            messages
                .iter()
                .all(|m| !m.to_lowercase().contains("cannot find")
                    && !m.to_lowercase().contains("unknown")),
            "no downstream item should be lost to the steer; got: {messages:#?}"
        );
    }
}

// --- The recovery bar (editing-dx.md §8, S1) -----------------------------------
//
// The three clauses the survey wrote as the statement/item synchronizer's
// acceptance test, as EXACT-COUNT pins. §12 records why they are new: "the pins
// assert 'at least one error', never an exact count", and §8's bar "cannot be
// defended without exact counts".
//
//   1. One missing token produces exactly one diagnostic. (Met before S1 too —
//      P6/P14 — so this half pins what S1 must not break.)
//   2. N independent errors produce N diagnostics, including in the SAME body and
//      including when the first is an unclosed delimiter.
//   3. A parse error never removes a diagnostic from a region it does not
//      contain. At the parse level that reads: the statements and items around a
//      broken one are still in the tree. (The analyze-level half — that their
//      DIAGNOSTICS survive — is pinned in `vilan-core/tests/inference/` and,
//      for the editor, in `vilan-lsp/src/document.rs`.)

/// Each diagnostic of a recovered parse as `(rendered message, the source text it
/// spans)` — the pair a user sees: what it says, and what it underlines.
fn diagnostics(source: &str) -> Vec<(String, String)> {
    let (_tree, errors) = parsing::parse(source);
    errors
        .iter()
        .map(|error| {
            (
                parsing::render(error),
                source[error.span.start..error.span.end].to_string(),
            )
        })
        .collect()
}

/// The recovered tree's `Debug`, for shape assertions.
fn tree_of(source: &str) -> String {
    let (tree, _errors) = parsing::parse(source);
    format!("{:?}", tree.expect("recovery always yields a tree"))
}

// --- Clause 1: one missing token, one diagnostic -------------------------------

#[test]
fn one_missing_semicolon_reports_exactly_one_diagnostic() {
    // P6, with the four correct statements after it. The count is the pin; the
    // message and anchor are S2's (`editing-dx.md` §4.4) — the gap after the
    // statement that lost its `;`, not the head of the one that follows.
    let source = "fun main() {\n\tlet a: i32 = 1\n\tlet b: i32 = 2;\n\tlet c: i32 = 3;\n\tlet d: i32 = 4;\n\tlet e: i32 = 5;\n}\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        1,
        "one missing `;`, one diagnostic: {reported:#?}"
    );
    assert_eq!(
        reported[0],
        (
            "expected `;` to end this statement".to_string(),
            "1".to_string()
        ),
        "the diagnostic names `;` and anchors at the gap"
    );
}

#[test]
fn one_unclosed_paren_reports_exactly_one_diagnostic() {
    // P14: one unclosed `(`, three correct statements after it. Before S1 the
    // count was 1 because the parse STOPPED (§4.3 — "the recovery bar is met by
    // the wrong means"); it is 1 now because recovery resumes at the next
    // statement boundary, which the sibling pins below prove.
    //
    // The message is the located one, not `unclosed \`(\``: `let b: i32 = 2` reads
    // as an argument, so a committed demand — the argument list's own `,`/`)` —
    // fails INSIDE the region and says where. That is the same shape, and the
    // same message, as §5.1's P8, which the survey grades the best in the survey
    // and asks to leave alone.
    let source =
        "fun main() {\n\tprint(\n\tlet b: i32 = 2;\n\tlet c: i32 = 3;\n\tlet d: i32 = 4;\n}\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        1,
        "one unclosed `(`, one diagnostic: {reported:#?}"
    );
    assert_eq!(
        reported[0],
        ("found ';' expected ',' or ')'".to_string(), ";".to_string()),
    );
    // …and the statements past the `;` that ended the unfinished region are
    // parsed, rather than the whole body being dropped.
    let tree = tree_of(source);
    assert!(tree.contains("\"c\"") && tree.contains("\"d\""), "{tree}");
}

#[test]
fn an_unfinished_call_recovers_the_statement_swallowed_into_it() {
    // The residual §15.8 recorded honestly and §17.5 closes: the statement
    // written where `print(`'s arguments were expected — `let swallowed:
    // i32 = 2` — is read as an argument first, same as before, but is no
    // longer lost when that reading is abandoned. `recover_statement`'s own
    // scan already boxes the region exactly (`opener` to `resume`, the `;`
    // that ends the unfinished call); retried from `opener + 1` as an
    // ordinary statement, it parses cleanly and lands exactly on `resume`,
    // so it is kept rather than skipped. The diagnostic is UNCHANGED — this
    // is the located `found ';' expected ',' or ')'` shape §5.1/§15.4 grade
    // best and ask to leave alone — only the recovered TREE gained a node.
    let source = "fun main() {\n\tlet above: i32 = 0;\n\tprint(\n\tlet swallowed: i32 = 2;\n\tlet below: i32 = 3;\n}\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        1,
        "one unfinished call, one diagnostic, exactly as before: {reported:#?}"
    );
    assert_eq!(
        reported[0],
        ("found ';' expected ',' or ')'".to_string(), ";".to_string()),
    );
    let tree = tree_of(source);
    assert!(
        tree.contains("\"above\""),
        "the statement above survives: {tree}"
    );
    assert!(
        tree.contains("\"swallowed\""),
        "the statement read as an argument is recovered, not lost: {tree}"
    );
    assert!(
        tree.contains("\"below\""),
        "the statement below survives: {tree}"
    );
}

#[test]
fn a_swallowed_statement_recovers_at_its_own_span_not_the_calls() {
    // The recovered node is `swallowed`'s OWN statement, at its own span —
    // not a copy of the abandoned call, and not shifted to start at the
    // call's `(`. `let swallowed: i32 = 2` starts at byte 43 in this source
    // (right after the newline and tab following `print(`).
    let source = "fun main() {\n\tprint(\n\tlet swallowed: i32 = 2;\n\tlet below: i32 = 3;\n}\n";
    let tree = tree_of(source);
    let expected_start = source.find("let swallowed").unwrap();
    assert!(
        tree.contains(&format!(", {expected_start}..")),
        "the recovered statement's span starts at `let swallowed`'s own position ({expected_start}): {tree}"
    );
}

#[test]
fn a_swallowed_statement_only_recovers_when_it_lands_exactly_on_resume() {
    // The safety net: when the abandoned region is NOT cleanly "one
    // statement" — here, a real first argument (`1`) followed by `,` and
    // then the statement-shaped second "argument" — retrying from
    // `opener + 1` as ONE statement fails immediately (`1` wants `;` next
    // and finds `,`), so the retry is discarded and the region is skipped
    // exactly as it was before this fix, rather than guessing which part
    // of it to keep.
    let source = "fun main() {\n\tprint(1,\n\tlet b: i32 = 2;\n\tlet below: i32 = 3;\n}\n";
    let tree = tree_of(source);
    assert!(
        !tree.contains("\"b\""),
        "the region isn't cleanly one statement, so nothing is guessed at: {tree}"
    );
    assert!(
        tree.contains("\"below\""),
        "the statement after the unfinished call still survives: {tree}"
    );
}

#[test]
fn a_missing_semicolon_keeps_the_statement_it_should_have_terminated() {
    // The statement parsed perfectly; only its terminator is absent, and the
    // token after it can only begin a new one. Dropping it would unbind `origin`
    // at every use below — a screenful of "cannot find" on correct lines, from a
    // statement the parser read without difficulty.
    let source =
        "fun main() {\n\tlet origin: i32 = 3\n\tlet total: i32 = origin + 1;\n\tprint(total);\n}\n";
    let reported = diagnostics(source);
    assert_eq!(reported.len(), 1, "{reported:#?}");
    let tree = tree_of(source);
    // The BINDING, not merely the name — `origin + 1` below mentions it either
    // way, so a substring pin on the bare name would pass on a dropped statement.
    assert!(
        tree.contains("Let((\"origin\""),
        "the statement missing its `;` is kept, not skipped: {tree}"
    );
    assert!(
        tree.contains("Let((\"total\""),
        "and so is the one after it: {tree}"
    );
}

#[test]
fn a_missing_semicolon_on_an_import_keeps_the_import() {
    // The same, for the two non-expression statements that take a terminator.
    // P3's file used to lose its `import` — so every use of `print` below
    // reported "cannot find 'print' in this scope" as well.
    let source = "import std::print

fun main() {\n\tprint(1);\n}\n";
    let reported = diagnostics(source);
    assert_eq!(reported.len(), 1, "{reported:#?}");
    assert!(
        tree_of(source).contains("Import"),
        "the import survives its missing `;`"
    );
}

#[test]
fn a_statement_that_is_not_merely_missing_its_semicolon_is_still_skipped() {
    // The boundary condition that keeps insertion from cascading: `1` is not a
    // statement anyone wrote, so `print 1);` takes the skipping path and reports
    // once — §5.2's accepted outcome for a missing OPENING paren. Two broken
    // statements report twice, one each, and neither multiplies.
    let reported = diagnostics("fun main() {\n\tprint 1);\n\tfoo bar baz;\n}\n");
    assert_eq!(reported.len(), 2, "one per broken statement: {reported:#?}");
    assert!(
        reported
            .iter()
            .all(|(message, _)| message == "expected `;` to end this statement"),
        "{reported:#?}"
    );
}

#[test]
fn a_missing_semicolon_on_a_void_bodys_last_statement_stays_silent() {
    // P5, pinned as a NON-diagnostic: the statement legally becomes the block's
    // tail expression, and a void tail in a void function is fine
    // (`ret-checking.md` rule 3). S2 must not turn correct language semantics
    // into an error.
    let source = "fun main() {\n\tlet a: i32 = 1;\n\ta\n}\n";
    assert_eq!(diagnostics(source), Vec::new(), "no `;` is wanted here");
}

// --- Clause 2: N errors, N diagnostics -----------------------------------------

#[test]
fn two_missing_semicolons_in_two_bodies_report_two() {
    // P7a — met before S1 (the bodies are independent) and pinned so it stays met.
    let source = "fun one() {\n\tlet a: i32 = 1\n\tprint(a);\n}\nfun two() {\n\tlet b: i32 = 2\n\tprint(b);\n}\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        2,
        "two bodies, two diagnostics: {reported:#?}"
    );
    assert!(
        reported
            .iter()
            .all(|(message, _)| message == "expected `;` to end this statement"),
        "{reported:#?}"
    );
}

#[test]
fn two_broken_statements_in_one_body_report_two() {
    // The half P7 could NOT reach before S1: two independent errors in the SAME
    // body. The first used to eat the whole body (mechanism 2), so the second was
    // never seen.
    let source = "fun main() {\n\tlet a: i32 = 1\n\tprint(a);\n\tlet b: i32 = 2\n\tprint(b);\n}\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        2,
        "two statements, two diagnostics: {reported:#?}"
    );
    assert!(
        reported
            .iter()
            .all(|(message, _)| message == "expected `;` to end this statement"),
        "{reported:#?}"
    );
}

#[test]
fn an_unclosed_delimiter_first_does_not_hide_a_later_error() {
    // Clause 2's second half, and the P31-B shape at the parse level: an unclosed
    // `(` above, a missing `;` below. Before S1 the unclosed region defeated
    // recovery and everything after it was dropped — one diagnostic, and the
    // file tail unparsed.
    let source = "fun one() {\n\tprint(\n}\nfun two() {\n\tlet b: i32 = 2\n\tprint(b);\n}\n";
    let reported = diagnostics(source);
    assert_eq!(reported.len(), 2, "both errors are reported: {reported:#?}");
    assert_eq!(reported[0].0, "unclosed `(`: expected a matching `)`");
    assert_eq!(reported[1].0, "expected `;` to end this statement");
}

// --- Clause 3: a parse error never removes what it does not contain ------------

#[test]
fn a_broken_statement_keeps_its_siblings_in_the_body() {
    // Mid-statement, with correct statements on BOTH sides: the broken one is
    // dropped and nothing else is. Before S1 `parse_block` replaced the entire
    // body with an EMPTY block (§2.2 mechanism 2), so `before` and `after` left
    // the tree with it.
    let tree = tree_of(
        "fun main() {\n\tlet before: i32 = 1;\n\tlet broken: i32 = ;\n\tlet after: i32 = 2;\n}\n",
    );
    assert!(
        tree.contains("\"before\""),
        "the statement above survives: {tree}"
    );
    assert!(
        tree.contains("\"after\""),
        "the statement below survives: {tree}"
    );
}

#[test]
fn a_broken_statement_keeps_the_items_below_it() {
    // The file-tail half: an item after a broken one still parses. Before S1 both
    // statement loops `break`, so everything below the first decline was dropped.
    let tree = tree_of(
        "fun broken() {\n\tprint(\n}\nfun below(): i32 {\n\t7\n}\nstruct After { x: i32 }\n",
    );
    assert!(
        tree.contains("\"below\""),
        "the item below survives: {tree}"
    );
    assert!(
        tree.contains("\"After\""),
        "and so does the one after it: {tree}"
    );
}

#[test]
fn an_unclosed_paren_in_an_item_header_keeps_the_items_below_it() {
    // The reach an item keyword gets that a statement head does not: `fun broken(`
    // never closes, so the scan would otherwise run to end of input, swallowing
    // the whole file inside the unfinished parameter list — the file-tail blackout
    // again, one layer up. `fun` cannot appear inside a parenthesized region
    // (only inside a block within one, which the scan tracks), so it ends the
    // region.
    let source =
        "fun broken( {\n\tprint(1);\n}\nfun below(): i32 {\n\t7\n}\nstruct After { x: i32 }\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        1,
        "one broken header, one diagnostic: {reported:#?}"
    );
    let tree = tree_of(source);
    assert!(
        tree.contains("\"below\""),
        "the item below survives: {tree}"
    );
    assert!(tree.contains("\"After\""), "and the one after it: {tree}");
}

#[test]
fn a_nested_item_inside_an_unfinished_call_is_not_a_boundary() {
    // The other side of that rule: a `fun` declared inside a closure body is
    // ordinary code, so it must NOT end the region — the `{` above it says so —
    // and neither does the `;` of a statement in that body. Stopping at either
    // would resume mid-expression and report a second time. One unfinished call,
    // one diagnostic, and the item after the enclosing function still parses.
    let source = "fun main() {\n\tapply(|| {\n\t\tfun helper() {}\n\t\tlet a: i32 = 1;\n\t}\n}\nfun below(): i32 {\n\t7\n}\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        1,
        "one unfinished call, one diagnostic: {reported:#?}"
    );
    let tree = tree_of(source);
    assert!(
        tree.contains("\"below\""),
        "the item after the broken one survives: {tree}"
    );
}

#[test]
fn a_broken_statement_in_a_nested_block_keeps_its_enclosing_body() {
    // Nested: the unclosed `(` is two blocks deep. The `}` that stops the scan is
    // the INNER block's, so the `if`'s body closes, the outer body keeps going,
    // and the statement after the `if` survives.
    let source = "fun main() {\n\tlet before: i32 = 1;\n\tif before > 0 {\n\t\tprint(\n\t}\n\tlet after: i32 = 2;\n}\n";
    let reported = diagnostics(source);
    assert_eq!(
        reported.len(),
        1,
        "one unclosed `(`, one diagnostic: {reported:#?}"
    );
    assert_eq!(reported[0].0, "unclosed `(`: expected a matching `)`");
    let tree = tree_of(source);
    assert!(
        tree.contains("\"before\""),
        "the statement above survives: {tree}"
    );
    assert!(
        tree.contains("\"after\""),
        "the statement after the `if` survives: {tree}"
    );
}

// --- The anchors the survey asked for ------------------------------------------

#[test]
fn a_file_scope_missing_semicolon_reports_at_the_gap() {
    // P3: an `import` with no `;`. Before S2 this read `found 'import' expected an
    // expression` ON the `import` keyword — §4.1 calls it incomprehensible, and
    // §4.2 records the fallback it came from as untested anywhere in the repo.
    let source = "import std::print\n\nfun main() {\n\tprint(1);\n}\n";
    let reported = diagnostics(source);
    assert_eq!(reported.len(), 1, "{reported:#?}");
    assert_eq!(
        reported[0],
        (
            "expected `;` to end this statement".to_string(),
            "t".to_string()
        ),
        "the anchor is the last character of `print`, where the `;` goes"
    );
}

#[test]
fn an_unclosed_brace_anchors_at_the_opening_brace() {
    // P13: a body that runs out of input. Before S1 this reported `found end of
    // input expected an expression` at EOF and the unclosed `{` — five lines up —
    // was never mentioned; the whole file was dropped with it.
    let source = "fun main() {\n\tlet x: i32 = 1;\n\tprint(x);\n";
    let reported = diagnostics(source);
    assert_eq!(reported.len(), 1, "{reported:#?}");
    assert_eq!(
        reported[0],
        (
            "unclosed `{`: expected a matching `}`".to_string(),
            "{".to_string()
        )
    );
    let tree = tree_of(source);
    assert!(
        tree.contains("\"x\""),
        "the body's statements survive: {tree}"
    );
}

#[test]
fn a_committed_separator_failure_still_reports_where_it_broke() {
    // §5.1's "best diagnostics in the survey" are NOT re-anchored: when a
    // committed demand fails INSIDE an unfinished region, its located message
    // wins over naming the opener. `distance(1, 2;` closes nothing either, so
    // only the failure-within rule keeps it from becoming "unclosed `(`".
    let source = "fun main() {\n\tlet total: i32 = distance(1, 2;\n}\n";
    let reported = diagnostics(source);
    assert_eq!(reported.len(), 1, "{reported:#?}");
    assert_eq!(
        reported[0],
        ("found ';' expected ',' or ')'".to_string(), ";".to_string())
    );
}
