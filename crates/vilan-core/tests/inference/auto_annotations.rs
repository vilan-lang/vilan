//! B570 — `auto` annotations, types the toolchain writes and keeps current
//! (`proposal/auto-annotations.md`, its nine questions RULED).
//!
//! One subject module of the `inference` test binary; the harness it is written
//! against lives in `support.rs`.
//!
//! `auto T` is a SIGNATURE, not a constraint (Q1): the body is inferred as if
//! the annotation were absent, everything outside reads `T` (Q2, stale or
//! not), and `check` refuses a `T` the inference no longer agrees with, the
//! rewrite carried at the end of the message for `vilan check --fix`. A bare
//! `auto` promised nothing: a warning with the fill (Q4). Long stage types are
//! written as the inlay hint shows them, a hinted node as its bare trait, and
//! checked as B161 checks one (Q3, door (b)). Every one of these programs was
//! a parse error before B570 (`auto` named a type).

use crate::support::*;

#[test]
fn b570_an_auto_return_that_agrees_checks_and_runs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun five(): auto i32 {
            5
        }

        fun pair(): auto (str, List<i32>) {
            ("a", [1, 2])
        }

        fun main() {
            print(five());
            print(pair().1.len());
        }
        "#,
        "5\n2\n",
    );
}

/// Output only (§7): the written type never directs the body — `5` under
/// `auto f64` is an `i32`, so the annotation is STALE rather than a float.
#[test]
fn b570_a_stale_auto_is_refused_with_its_rewrite_and_never_directs_the_body() {
    assert_fails_with(
        r#"
        fun ratio(): auto f64 {
            5
        }

        fun main() {
            let _ = ratio();
        }
        "#,
        "stale `auto`: `ratio` now returns `i32`, not the written `auto f64`, and its 1 caller \
         was checked against it — `vilan check --fix` writes `auto i32`",
    );
}

/// Q2 (RULED): callers read the WRITTEN `T` while it is stale — a caller that
/// agrees with `T` is clean, one that agrees only with the body is refused.
#[test]
fn b570_callers_are_checked_against_the_written_type_while_it_is_stale() {
    let diagnostics = failure_diagnostics(
        r#"
        fun label(): auto i32 {
            "text"
        }

        fun main() {
            let number: i32 = label();
            let text: str = label();
            let _ = (number, text);
        }
        "#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .map(|(message, _)| message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("stale `auto`: `label` now returns `str`")),
        "{messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Expected str, but got i32")),
        "the caller expecting the body's type is checked against `auto i32`: {messages:#?}"
    );
    assert_eq!(
        messages.len(),
        2,
        "the agreeing caller is clean: {messages:#?}"
    );
}

/// Q4 (RULED): a bare `auto` is a warning carrying the fill, and its callers
/// read the inferred type through it.
#[test]
fn b570_a_bare_auto_warns_with_the_fill_and_reads_through() {
    let source = r#"
        import std::io::print;

        fun greeting(): auto {
            "hello"
        }

        fun main() {
            let text: str = greeting();
            let count: auto = 3;
            print(i"{text} {count}");
        }
    "#;
    assert_compiles_and_runs(source, "hello 3\n");
    let warnings = warnings(source);
    assert!(
        warnings.contains(
            &"unfilled `auto`: `greeting` returns `str` — `vilan check --fix` writes `auto str`"
                .to_string()
        ),
        "{warnings:#?}"
    );
    assert!(
        warnings.contains(
            &"unfilled `auto`: `count` is `i32` — `vilan check --fix` writes `auto i32`"
                .to_string()
        ),
        "{warnings:#?}"
    );
}

/// u03's shape locked: a module binding whose type a USE elsewhere decided is
/// fixed by its written `auto T` — the use is checked against it.
#[test]
fn b570_a_module_binding_locked_by_auto_refuses_a_widening_use() {
    assert_compiles(
        r#"
        mut names: auto List<str> = [];

        fun main() {
            names.push("a");
        }
        "#,
    );
    assert_fails_with(
        r#"
        mut names: auto List<str> = [];

        fun main() {
            names.push(5);
        }
        "#,
        "Expected str, but got i32 instead",
    );
}

/// Q3 (RULED, door (b)): a bare trait in an `auto` type is checked as B161
/// checks one at a binding — the inferred type implements it — and the
/// carried type is still compared.
#[test]
fn b570_a_stage_type_is_written_as_its_bare_trait_and_checked_wide() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{SignalCell, Pipe};

        fun doubled(cell: SignalCell<i32>): auto Pipe<i32> {
            cell.derive(|value| value * 2)
        }

        fun main() {
            print(doubled(SignalCell::new(3)).sample());
        }
        "#,
        "6\n",
    );
    assert_fails_with(
        r#"
        import std::reactive::{SignalCell, Pipe};

        fun doubled(cell: SignalCell<i32>): auto Pipe<str> {
            cell.derive(|value| value * 2)
        }

        fun main() {
            let _ = doubled(SignalCell::new(3));
        }
        "#,
        "stale `auto`: `doubled` now returns `Derive<SignalCell<i32>, i32, i32>`, not the \
         written `auto Pipe<str>`",
    );
}

/// The fix writes what the inlay hint shows: a stale stage type is rewritten
/// to its bare trait, named as the file can name it.
#[test]
fn b570_a_stale_stage_type_is_rewritten_to_the_bare_trait() {
    assert_fails_with(
        r#"
        import std::reactive::{SignalCell, Pipe};

        fun labels(cell: SignalCell<i32>): auto Pipe<i32> {
            cell.derive(|value| i"{value}")
        }

        fun main() {
            let _ = labels(SignalCell::new(3));
        }
        "#,
        "— `vilan check --fix` writes `auto Pipe<str>`",
    );
}

/// Q7 (RULED): a type is written the shortest way the FILE can name it and
/// never by adding an import; a type it cannot name is declined with the
/// import that would let it.
#[test]
fn b570_a_type_the_file_cannot_name_is_declined_with_the_import() {
    let warnings = warnings(
        r#"
        import std::reactive::SignalCell;

        fun doubled(cell: SignalCell<i32>): auto {
            cell.derive(|value| value * 2)
        }

        fun main() {
            let _ = doubled(SignalCell::new(3));
        }
        "#,
    );
    assert!(
        warnings.iter().any(|warning| warning.starts_with(
            "unfilled `auto`: `doubled` returns `Derive<SignalCell<i32>, i32, i32>` — write the \
             type by hand: this file can reach `Pipe` neither by name nor through a module it \
             imports"
        )),
        "{warnings:#?}"
    );
}

/// §2: `auto` stands only where a type is inferred.
#[test]
fn b570_auto_is_refused_where_nothing_is_inferred() {
    for source in [
        "fun take(value: auto i32) {}\nfun main() {}\n",
        "struct Box {\n\tvalue: auto i32,\n}\nfun main() {}\n",
        "fun main() {\n\tlet values: List<auto> = [];\n}\n",
    ] {
        assert_fails_with(source, "nothing is inferred here, so write the type");
    }
    assert_fails_with(
        "trait Named {\n\tfun name(self): auto str;\n}\nfun main() {}\n",
        "a trait member's return is the trait's",
    );
}

/// A local binding takes `auto` too (Q8's slice, built with the rest): the
/// initializer is inferred without it, and a stale one is refused.
#[test]
fn b570_a_local_auto_is_checked_like_a_module_bindings() {
    assert_compiles(
        r#"
        fun main() {
            let words: auto List<str> = ["a"];
            let _ = words;
        }
        "#,
    );
    assert_fails_with(
        r#"
        fun main() {
            let count: auto str = 4;
            let _ = count;
        }
        "#,
        "stale `auto`: `count` is now `i32`, not the written `auto str`, and its uses were \
         checked against it — `vilan check --fix` writes `auto i32`",
    );
}

/// `auto` is contextual: a name everywhere else, `auto::` a path.
#[test]
fn b570_auto_is_still_a_name_where_a_name_can_stand() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun auto(value: i32): i32 {
            value + 1
        }

        fun main() {
            let auto_value = auto(1);
            print(auto_value);
        }
        "#,
        "2\n",
    );
}

/// A labelled tuple (B569) is filled with its labels — they are no part of
/// the type, but they are what its callers name.
#[test]
fn b570_a_bare_auto_fills_a_labelled_tuple_with_its_labels() {
    let source = r#"
        import std::io::print;

        fun origin(): auto {
            (x = 1, y = 2)
        }

        fun main() {
            print(origin().y);
        }
    "#;
    assert_compiles_and_runs(source, "2\n");
    let warnings = warnings(source);
    assert!(
        warnings.contains(
            &"unfilled `auto`: `origin` returns `(x: i32, y: i32)` — `vilan check --fix` writes \
              `auto (x: i32, y: i32)`"
                .to_string()
        ),
        "{warnings:#?}"
    );
}

/// Labels agree as the language reconciles them (B569 §4.3): by position, a
/// set against none, two disjoint sets — so a written labelled tuple over an
/// unlabelled body, or the reverse, is current, and callers read the written
/// labels.
#[test]
fn b570_tuple_labels_that_reconcile_agree() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun origin(): auto (x: i32, y: i32) {
            (1, 2)
        }

        fun corner(): auto (i32, i32) {
            (x = 3, y = 4)
        }

        fun spot(): auto (row: i32, column: i32) {
            (x = 5, y = 6)
        }

        fun main() {
            print(origin().x);
            print(corner().1);
            print(spot().column);
        }
        "#,
        "1\n4\n6\n",
    );
}

/// A label both sides carry at different slots does not reconcile: callers
/// reading the written `y` would get the slot the body calls `x`. Stale, and
/// the rewrite writes the body's labels.
#[test]
fn b570_a_contradicting_tuple_label_is_stale() {
    let diagnostics = failure_diagnostics(
        r#"
        fun origin(): auto (y: i32, x: i32) {
            (x = 1, y = 2)
        }

        fun main() {
            let _ = origin();
        }
        "#,
    );
    assert!(
        diagnostics.iter().any(|(message, _)| message.starts_with(
            "stale `auto`: `origin` now returns `(x: i32, y: i32)`, not the written \
             `auto (y: i32, x: i32)`"
        ) && message
            .ends_with("— `vilan check --fix` writes `auto (x: i32, y: i32)`")),
        "{diagnostics:#?}"
    );
}
