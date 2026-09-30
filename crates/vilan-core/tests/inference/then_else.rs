//! B459 — the `then`/`else` forms, sugar over `if`.
//!
//! One subject module of the `inference` test binary; the harness it is written
//! against lives in `support.rs`.
//!
//! The expression form `c then a else b` and the statement forms `c then S;`,
//! `c else S;` (the guard) and `c then S else S;` parse to the `if` they spell,
//! so these pins hold what the READING changes: a value needs both branches
//! and unifies them, a statement discards each branch's value, and the `is`
//! bindings of the condition reach the `then` branch and not the `else` (B171,
//! as an `if`'s do). The guard's bindings reaching past the statement (R15)
//! are the analyzer's, pinned with that rule.

use crate::support::*;

#[test]
fn b459_the_expression_form_is_a_value_with_both_branches() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun sign(n: i32): str {
            n < 0 then "negative" else n == 0 then "zero" else "positive"
        }

        fun main() {
            let verbose = true;
            let label = verbose then "loud" else "quiet";
            let pick = |x: bool| x then 1 else 2;
            let a = true;
            let b = false;
            mut chosen = "";
            chosen = a && b then "both" else "not both";
            print(i"{label} {sign(-3)} {sign(0)} {sign(9)} {pick(false)} {chosen}");
            print(1 + (a then 10 else 20));
        }
        main();
        "#,
        "loud negative zero positive 2 not both\n11\n",
    );
}

/// Q6: at statement position each branch is a statement — `print` and an
/// `i32` assignment, or a `str`-returning call and a `ret` — so the branches
/// need not unify; and each statement form runs the branch it names.
#[test]
fn b459_the_statement_forms_discard_their_branches_values() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun loud(word: str): str {
            print(word);
            word
        }

        fun first_positive(xs: List<i32>): i32 {
            for x in xs {
                x > 0 then ret x;
            }
            -1
        }

        fun main() {
            let ready = true;
            mut count = 0;
            ready then loud("then ran") else count += 1;
            !ready then loud("never") else count += 10;
            ready else print("the guard skips");
            !ready else print("the guard ran");
            ready then print("bare then");
            ready then !ready then print("inner") else print("inner else");
            print(i"{count} {first_positive([ -1, 0, 5 ])}");
        }
        main();
        "#,
        "then ran\nthe guard ran\nbare then\ninner else\n10 5\n",
    );
}

/// The guard's canonical use: `jump continue` / `ret` in its branch.
#[test]
fn b459_the_guard_diverges_out_of_a_loop_and_a_function() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun half(n: i32): i32 {
            n % 2 == 0 else ret -1;
            n / 2
        }

        fun main() {
            mut total = 0;
            for x in [ 1, 2, 3, 4 ] {
                x % 2 == 0 else jump continue;
                total += x;
            }
            print(i"{total} {half(8)} {half(3)}");
        }
        main();
        "#,
        "6 4 -1\n",
    );
}

/// The condition's `is` bindings reach the `then` branch (B171, the `if`'s
/// rule — the sugar is a pure rewrite there), in both readings.
#[test]
fn b459_an_is_binding_reaches_the_then_branch() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let found: Option<i32> = Some(4);
            found is Some(let n) then print(i"got {n}") else print("none");
            let doubled = found is Some(let m) then m * 2 else 0;
            print(doubled);
        }
        main();
        "#,
        "got 4\n8\n",
    );
    // …and not the `else` branch.
    assert_fails_with(
        r#"
        import std::io::print;
        fun main() {
            let found: Option<i32> = None;
            found is Some(let n) then print("some") else print(i"{n}");
        }
        main();
        "#,
        "cannot find 'n'",
    );
}

/// As a VALUE the form unifies its branches exactly as an `if` does.
#[test]
fn b459_a_value_form_unifies_its_branches() {
    assert_fails(
        r#"
        fun main() {
            let ready = true;
            let x: i32 = ready then 1 else "two";
        }
        main();
        "#,
    );
}

/// Q3 (RULED): no bare `then` in value position — and the guard, which has no
/// value to give, is a statement only. Q8: a `let` branch is refused.
#[test]
fn b459_the_refusals_name_the_spelling_they_want() {
    assert_fails_once_with(
        "fun main() {\n\tlet ready = true;\n\tlet x = ready then 1;\n}\n",
        "a `then` used as a VALUE needs its `else`",
    );
    assert_fails_once_with(
        "fun main() {\n\tlet ready = true;\n\tready then let x = 1;\n}\n",
        "a `then`/`else` branch is one statement with no block of its own",
    );
    assert_fails_with(
        "fun main() {\n\tlet ready = true;\n\tlet x = ready else 1;\n}\n",
        "expected `;`",
    );
}

/// `then` is contextual: a name everywhere but after a complete operand.
#[test]
fn b459_then_is_a_name_everywhere_but_after_an_operand() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Promise { then: i32 }

        impl Promise {
            fun then(self, then: i32): i32 { self.then + then }
        }

        fun then(then: i32): i32 { then * 2 }

        fun main() {
            let doubled = then(1);
            let then = 3;
            let promise = Promise { then = then };
            let ready = true;
            print(ready then promise.then(doubled) else 0);
        }
        main();
        "#,
        "5\n",
    );
}

// --- R15 (RULED 2026-09-29): the guard's bindings reach the rest of the block --
//
// `opt is Some(let v) else ret;` is `if opt is Some(let v) {} else { ret; }`,
// where B171 scopes `v` to the empty then-block. When the `else` statement
// DIVERGES — `ret`, `jump`, a `panic(..)`, an endless loop (B204's four
// leaves) — the only way past the statement is the condition's TRUE path, so
// the captures that path binds are published into the enclosing block from
// the statement's end: Swift's `guard`, Rust's `let … else`. B187's shape,
// mirrored (there, a diverging THEN publishes the FALSE-path captures).

#[test]
fn r15_a_guards_binding_reaches_the_rest_of_the_block() {
    assert_compiles_and_runs(
        r#"
        import std::io::{ panic, print };

        fun describe(found: Option<i32>): str {
            found is Some(let n) else ret "none";
            i"got {n}"
        }

        fun total(xs: List<Option<i32>>): i32 {
            mut sum = 0;
            for x in xs {
                x is Some(let value) else jump continue;
                sum += value;
            }
            sum
        }

        fun must(found: Option<str>): str {
            found is Some(let text) else panic("missing");
            text
        }

        fun main() {
            print(describe(Some(4)));
            print(describe(None));
            print(total([ Some(1), None, Some(5) ]));
            print(must(Some("here")));
            // The `then` + `else` statement form publishes too.
            let pair: Option<i32> = Some(2);
            pair is Some(let p) then print("present") else ret;
            print(p * 10);
        }
        main();
        "#,
        "got 4\nnone\n6\nhere\npresent\n20\n",
    );
}

/// A NON-diverging `else` proves nothing about the path past the statement:
/// the capture stays the then-block's (B171), and a read after it misses.
#[test]
fn r15_a_non_diverging_else_does_not_extend_the_binding() {
    assert_fails_with(
        r#"
        import std::io::print;
        fun main() {
            let found: Option<i32> = Some(1);
            found is Some(let n) else print("none");
            print(n);
        }
        main();
        "#,
        "cannot find 'n'",
    );
    // Nor does the keyword `if` it spells — R15 is the sugar's rule.
    assert_fails_with(
        r#"
        import std::io::print;
        fun main() {
            let found: Option<i32> = Some(1);
            if found is Some(let n) {} else { ret; }
            print(n);
        }
        main();
        "#,
        "cannot find 'n'",
    );
}

/// Only what the condition's TRUE path binds is published: a capture under a
/// negation or in one arm of `||` is not proven by the condition holding.
#[test]
fn r15_only_the_true_paths_captures_are_published() {
    assert_fails_with(
        r#"
        import std::io::print;
        fun main() {
            let a: Option<i32> = Some(1);
            let b: Option<i32> = None;
            a is Some(let x) || b is Some(let x) else ret;
            print(x);
        }
        main();
        "#,
        "cannot find 'x'",
    );
}

/// Nested and chained guards: each publishes into the block it is written in,
/// a later guard reads an earlier one's binding, and a guard inside a nested
/// block publishes there and no further.
#[test]
fn r15_nested_guards_each_publish_into_their_own_block() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun pick(outer: Option<Option<i32>>): i32 {
            outer is Some(let inner) else ret -1;
            inner is Some(let value) else ret -2;
            {
                value > 1 else ret 0;
                let scaled = value * 2;
                scaled is 4 else ret 3;
            }
            value
        }

        fun main() {
            print(pick(Some(Some(2))));
            print(pick(Some(None)));
            print(pick(None));
            print(pick(Some(Some(1))));
        }
        main();
        "#,
        "2\n-2\n-1\n0\n",
    );
    // A binding published inside a nested block ends with it.
    assert_fails_with(
        r#"
        import std::io::print;
        fun main() {
            let found: Option<i32> = Some(1);
            {
                found is Some(let n) else ret;
            }
            print(n);
        }
        main();
        "#,
        "cannot find 'n'",
    );
}
