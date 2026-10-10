//! B569 — named tuple fields (`named-tuple-fields.md`, RULED 2026-10-09).
//!
//! One subject module of the `inference` test binary; the harness it is written
//! against lives in `support.rs`.
//!
//! S1 takes assignment out of value position (§3.2): an assignment stands only
//! where its value is discarded — a statement, a block's tail, a `match` arm, a
//! closure's expression body, a `then`/`else` branch of a form standing at one
//! — and is refused, with the statement to write named, wherever its value
//! would be used. That is what frees `name = value` inside parentheses to be a
//! tuple's label.

use crate::support::*;

/// B569 S1: every discarded-value position keeps assignment, and the program
/// means what it meant before the rule.
#[test]
fn b569_assignment_runs_wherever_its_value_is_discarded() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut x = 0;
            x = 1;
            x += 1;
            let unit = { x = x * 10 };
            print(x);
            match x {
                20 => x = 3,
                _ => x = 4,
            }
            print(x);
            mut total = 0;
            let add = |v: i32| total += v;
            add(5);
            add(6);
            print(total);
            x > 2 then x = 7 else x = 8;
            print(x);
            x == 0 else x = 9;
            print(x);
            let pick = match x { 9 => x = 1, _ => x = 2 };
            print(x);
        }
        "#,
        "20\n3\n11\n7\n9\n1\n",
    );
}

/// B569 S1: an assignment whose value is used is refused at the parse, with
/// the statement it should have been.
#[test]
fn b569_an_assignment_used_as_a_value_is_refused() {
    for (body, message) in [
        (
            "let y = x = 5;",
            "an assignment is a statement and has no value: write `x = 5;` before this, and use `x`",
        ),
        (
            "let y = [x += 1];",
            "an assignment is a statement and has no value: write `x += 1;` before this, and use `x`",
        ),
        (
            "takes(x = 5);",
            "inside parentheses `x = …` is a tuple's label, not an assignment: to assign, write `x = 5;` before this, and use `x`",
        ),
        (
            "let y = true then x = 5 else 0;",
            "an assignment is a statement and has no value: write `x = 5;` before this, and use `x`",
        ),
    ] {
        let source =
            format!("fun takes(v: void) {{}}\nfun main() {{\n    mut x = 0;\n    {body}\n}}\n");
        assert_fails_with(&source, message);
    }
}
