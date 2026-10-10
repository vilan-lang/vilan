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

// --- S2: labels ---------------------------------------------------------------
//
// Labels are a slot of the tuple TYPE that unification carries and never
// compares (§10, Q1). A labelled literal matches a labelled type BY NAME
// (§4.1); labelled and unlabelled convert freely, and differently named labels
// reconcile by position (§4.2); a label both sides carry at different positions
// is refused (§4.3, Q2). Mono, the emitters and the contract hash erase them.

/// The owner's sketch (§0), as ruled: `point_3` is `(5, 7)` — by name — and
/// its entries are evaluated as WRITTEN, `y` before `x`.
#[test]
fn b569_the_owners_sketch_matches_by_name_and_evaluates_as_written() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun say(label: str, value: f64): f64 {
            print(label);
            value
        }

        fun main() {
            let point_1: (x: f64, y: f64) = (x = 5, y = 7);
            let point_2: (x: f64, y: f64) = (5, 7);
            let point_3: (x: f64, y: f64) = (y = say("y", 7), x = say("x", 5));
            print(i"point x={point_1.x}, y={point_1.y}");
            print(i"{point_2.x} {point_2.y}");
            print(i"{point_3.x} {point_3.y} {point_3.0} {point_3.1}");
        }
        "#,
        "y\nx\npoint x=5, y=7\n5 7\n5 7 5 7\n",
    );
}

/// With no expected type a labelled literal has its written order (§4.1).
#[test]
fn b569_a_labelled_literal_alone_keeps_its_written_order() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let q = (y = 7, x = 5);
            print(q.y);
            print(q.0);
            print(q.x);
        }
        "#,
        "7\n7\n5\n",
    );
}

/// `p.x` reads and writes the slot, through a `mut` binding, compound
/// assignment included; `p.0` keeps working beside it (§5).
#[test]
fn b569_a_label_reads_and_writes_its_slot() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut m: (a: i32, b: i32) = (1, 2);
            m.b = 10;
            m.a += 1;
            m.0 += 100;
            print(i"{m.a} {m.b}");
            let nested: (outer: (left: i32, right: i32), tag: str) = ((left = 3, right = 4), "t");
            print(nested.outer.right + nested.outer.0);
            let words: (type: str, if: bool) = (type = "kind", if = true);
            print(words.type);
            print(words.if);
        }
        "#,
        "102 10\n7\nkind\ntrue\n",
    );
}

/// §7, Q4: `(x = 5)` is the one-slot labelled tuple `(x: i32)`, distinct from
/// `i32`; `(5)` stays a group.
#[test]
fn b569_the_one_slot_labelled_tuple_is_a_tuple() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let one = (x = 5);
            let typed: (x: i32) = (x = 6);
            let group: i32 = (7);
            print(one.x + typed.x + group);
        }
        "#,
        "18\n",
    );
    assert_fails_with(
        "fun main() {\n    let n: i32 = (x = 5);\n}\n",
        "Expected i32, but got (x: i32) instead. `(x = 5)` is a tuple with the label `x`; to assign, write `x = 5;`",
    );
    assert_fails_with(
        "fun main() {\n    mut x = 0;\n    (x = 5);\n}\n",
        "`(x = 5)` is a tuple with the label `x`, and a statement discards it: to assign, write `x = 5;`",
    );
}

/// §4.2: labelled into unlabelled drops the labels, unlabelled into labelled
/// takes them, and differently named labels reconcile by position — at a
/// binding, a parameter and a return.
#[test]
fn b569_labelled_and_unlabelled_convert_freely() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun plain(pair: (f64, f64)): f64 { pair.0 - pair.1 }
        fun sized(size: (w: f64, h: f64)): f64 { size.w * size.h }
        fun origin(): (x: f64, y: f64) { (1, 2) }

        fun main() {
            let p: (x: f64, y: f64) = (x = 5, y = 7);
            let dropped: (f64, f64) = p;
            print(dropped.0);
            print(plain(p));
            print(sized(p));
            let renamed: (w: f64, h: f64) = p;
            print(renamed.h);
            print(plain((x = 9, y = 1)));
            print(origin().y);
        }
        "#,
        "5\n-2\n35\n7\n8\n2\n",
    );
}

/// §4.3, Q2: the same label set in another order is refused, and so is the
/// wider case — one label shared at another position — with both rewrites
/// spelled for a place (by name only where the value has every label).
#[test]
fn b569_a_label_at_another_position_is_refused() {
    let base = "fun main() {\n    let p: (x: f64, y: f64) = (x = 5, y = 7);\n";
    assert_fails_with(
        &format!("{base}    let q: (y: f64, x: f64) = p;\n}}\n"),
        "Expected (y: f64, x: f64), but got (x: f64, y: f64) instead: the label `x` names slot 0 of the value and slot 1 here, so the two do not convert — match by name, writing the labels out, or by position, dropping them: by name, `(y = p.y, x = p.x)`; by position, `(p.0, p.1)`",
    );
    assert_fails_with(
        &format!("{base}    let q: (y: f64, z: f64) = p;\n}}\n"),
        "the label `y` names slot 1 of the value and slot 0 here, so the two do not convert — match by name, writing the labels out, or by position, dropping them: by position, `(p.0, p.1)`",
    );
    assert_fails_with(
        &format!("fun takes(at: (y: f64, x: f64)) {{}}\n{base}    takes(p);\n}}\n"),
        "the label `x` names slot 0 of the value and slot 1 here",
    );
    // A value that is not a place would be evaluated once per slot: the
    // sentence stands alone.
    assert_fails_without(
        &format!(
            "fun make(): (x: f64, y: f64) {{ (x = 1, y = 2) }}\n{base}    let q: (y: f64, x: f64) = make();\n}}\n"
        ),
        "by position, `",
    );
    // Shared labels at the SAME positions agree.
    assert_compiles(&format!(
        "{base}    let q: (x: f64, y: f64) = p;\n    let r: (x: f64, h: f64) = p;\n}}\n"
    ));
}

/// §4.1: a labelled literal names exactly its landing type's labels.
#[test]
fn b569_a_labelled_literal_with_another_label_set_is_refused() {
    assert_fails_with(
        "fun main() {\n    let bad: (x: f64, y: f64) = (x = 1, z = 2);\n}\n",
        "`z` is not a label of `(x: f64, y: f64)`: a labelled tuple names exactly its type's labels, and this one leaves out `y`",
    );
}

/// §5: a label the tuple does not carry is named with the ones it does; an
/// unlabelled tuple keeps the positional sentence.
#[test]
fn b569_a_missing_label_names_the_labels_there_are() {
    assert_fails_with(
        "fun main() {\n    let p = (x = 1, y = 2);\n    let _z = p.z;\n}\n",
        "`(x: i32, y: i32)` has no label `z`; its labels are `x`, `y`",
    );
    assert_fails_with(
        "fun main() {\n    let p: (i32, i32) = (x = 1, y = 2);\n    let _x = p.x;\n}\n",
        "a tuple's members are its positions: `(i32, i32)` has no member 'x'",
    );
}

/// §2, Q5, Q10: every slot or none, each label once — written (the parser)
/// and through a spread (the tuple rule); a labelled spread concatenates.
#[test]
fn b569_labels_are_all_or_nothing_and_spreads_concatenate_them() {
    assert_fails_with(
        "fun main() {\n    let _m = (x = 1, 2);\n}\n",
        "a tuple labels every slot or none: label this one too",
    );
    assert_fails_with(
        "fun main() {\n    let _t: (x: f64, f64) = (1, 2);\n}\n",
        "a tuple labels every slot or none: label this one too",
    );
    assert_fails_with(
        "fun main() {\n    let _d = (x = 1, x = 2);\n}\n",
        "a tuple's labels name its slots, so each is written once",
    );
    assert_fails_with(
        "fun main() {\n    let pair = (1, 2);\n    let _s = (..pair, z = 3);\n}\n",
        "a tuple labels every slot or none, and a spread brings its operand's slots with their labels",
    );
    assert_fails_with(
        "fun main() {\n    let p = (x = 1, y = 2);\n    let _s = (..p, x = 3);\n}\n",
        "`x` labels two of these",
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let p = (x = 1, y = 2);
            let s = (..p, z = 3);
            print(s.x + s.y * 10 + s.z * 100);
        }
        "#,
        "321\n",
    );
}

/// §4.2: labels go through generics as part of the type a parameter binds,
/// and a function returning a labelled type matches its literal by name.
#[test]
fn b569_labels_ride_a_generic_parameter_and_a_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun id<T>(t: T): T { t }

        fun bounds(xs: List<i32>): (min: i32, max: i32) {
            mut low = xs[0];
            mut high = xs[0];
            for x in xs {
                x < low then low = x;
                x > high then high = x;
            }
            (max = high, min = low)
        }

        fun main() {
            let p: (x: f64, y: f64) = (x = 5, y = 7);
            print(id(p).y);
            let b = bounds([3, 1, 4, 1, 5]);
            print(i"{b.min}..{b.max}");
        }
        "#,
        "7\n1..5\n",
    );
}

/// §4.2: a join types a later arm against a labelled first one, so literal
/// arms and list elements match it by name.
#[test]
fn b569_a_join_matches_later_labelled_literals_by_name() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun pick(first: bool): (x: i32, y: i32) {
            first then (x = 1, y = 2) else (y = 3, x = 4)
        }

        fun main() {
            print(pick(true).x);
            print(pick(false).x);
            let n = 2;
            let m = match n { 1 => (x = 1, y = 2), _ => (y = 5, x = 6) };
            print(m.x);
            let rows = [(x = 1, y = 2), (y = 3, x = 4)];
            print(rows[1].x);
        }
        "#,
        "1\n4\n6\n4\n",
    );
}

/// §5: a mapped tuple keeps its source's labels — `combine((x = a, y = b))`
/// is a cell of `(x: i32, y: i32)`, and the derive reads `v.x`.
#[test]
fn b569_a_mapped_tuple_carries_its_sources_labels() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ SignalCell, combine, Source };

        fun main() {
            let a = SignalCell::new(1);
            let b = SignalCell::new(2);
            let both = combine((x = a, y = b));
            let sum = both.derive(|v| v.x * 10 + v.y);
            print(sum.sample());
        }
        "#,
        "12\n",
    );
}

/// Labels are not an identity: the tuple blankets (B443/B557 — `PartialEq`,
/// `Hashable`, `Debug`) still select for a labelled tuple, and a labelled
/// and an unlabelled one compare as one type.
#[test]
fn b569_the_tuple_blankets_select_for_a_labelled_tuple() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::hash_set::HashSet;
        import std::debug::Debug;

        fun show<T: Debug>(value: T): str { value.debug() }

        fun main() {
            let p: (x: i32, y: i32) = (x = 1, y = 2);
            print(p == (1, 2));
            print(p == (x = 1, y = 3));
            mut seen: HashSet<(x: i32, y: i32)> = HashSet::new();
            seen.insert(p);
            seen.insert((1, 2));
            print(seen.len());
            print(show(p));
        }
        "#,
        "true\nfalse\n1\n(1, 2)\n",
    );
}

/// Q9: an `impl` on a labelled tuple is refused with the steer to a struct.
#[test]
fn b569_an_impl_on_a_labelled_tuple_is_refused() {
    assert_fails_with(
        "impl (x: f64, y: f64) {\n    fun norm(self): f64 { self.0 }\n}\nfun main() {}\n",
        "an `impl` cannot name a labelled tuple: labels name positions and are not an identity",
    );
}

// --- S3: by name in patterns, and `dbg` -----------------------------------------

/// §2: a tuple pattern written by name — label, then what its slot meets —
/// places each element at its label's slot, in a `let`, a `match` arm, an
/// `is`, a `for` binder and the one-slot form; positional patterns ignore
/// labels as before.
#[test]
fn b569_a_pattern_destructures_by_name() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let p: (x: f64, y: f64) = (x = 5, y = 7);
            let (y = top, x = left) = p;
            print(i"{top} {left}");
            let (a, b) = p;
            print(a + b);
            let q = (x = 0, y = 3);
            match q {
                (x = 0, y = let v) => print(i"on the axis at {v}"),
                (x = let h, y = _) => print(h),
            }
            q is (y = 3, x = let found) then print(found);
            for (y = second, x = first) in [(x = 1, y = 2), (x = 3, y = 4)] {
                print(first * 10 + second);
            }
            let (x = only) = (x = 9);
            print(only);
            mut (b = ys, a = xs) = (a = 1, b = 2);
            xs += 10;
            print(xs + ys);
        }
        "#,
        "7 5\n12\non the axis at 3\n0\n12\n34\n9\n13\n",
    );
}

/// A by-name pattern names exactly its value's labels, over a labelled tuple.
#[test]
fn b569_a_by_name_pattern_is_checked_against_the_labels() {
    let base = "fun main() {\n    let p: (x: f64, y: f64) = (x = 5, y = 7);\n";
    assert_fails_with(
        &format!("{base}    let (y = top, z = left) = p;\n}}\n"),
        "`z` is not a label of `(x: f64, y: f64)`: a by-name pattern names exactly its value's labels, and this one leaves out `x`",
    );
    assert_fails_with(
        &format!("{base}    let (y = only) = p;\n}}\n"),
        "this pattern does not name every label of `(x: f64, y: f64)`",
    );
    assert_fails_with(
        "fun main() {\n    let (x = a, y = b) = (1, 2);\n}\n",
        "this pattern destructures by name, but `(i32, i32)` has no labels: destructure it by position, `(a, b)`",
    );
    assert_fails_once_with(
        &format!("{base}    let (x = c, d) = p;\n}}\n"),
        "a tuple labels every slot or none",
    );
}

/// §5: `dbg` prints a labelled tuple as its literal, nested and in a list —
/// and a tuple reached through a generic's substitution erased, whichever
/// label set reached the shared instance first (§6.3).
#[test]
fn b569_dbg_prints_labels_where_they_are_written() {
    let source = r#"
        fun show<T>(value: T) {
            dbg(value);
        }

        fun main() {
            let p: (x: f64, y: f64) = (x = 5, y = 7);
            let q: (f64, f64) = (1, 2);
            dbg(p);
            dbg(q);
            dbg([(a = 1, b = "one")]);
            dbg((outer = (left = 1, right = 2), tag = "t"));
            show(p);
            show(q);
        }
    "#;
    let stderr = compile_and_run_status(source).1;
    for line in [
        "p = (x = 5.0, y = 7.0)",
        "q = (1.0, 2.0)",
        "[(a = 1, b = \"one\")]",
        "(outer = (left = 1, right = 2), tag = \"t\")",
        "value = (5.0, 7.0)",
        "value = (1.0, 2.0)",
    ] {
        assert!(
            stderr.contains(line),
            "`dbg` printed {line:?}; got:\n{stderr}"
        );
    }
}
