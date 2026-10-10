//! B571 — type ascription, `value as T` (`proposal/type-ascription.md`).
//!
//! One subject module of the `inference` test binary; the harness it is written
//! against lives in `support.rs`.
//!
//! An ascription types its value exactly as an annotated `let` types its
//! initializer (§2, Q1 RULED): the type flows in, every coercion the binding
//! performs is performed, a mismatch is refused — and no binding is made, so
//! nothing is copied that the unascribed value would not copy, and the result
//! is a value, never a place (§7.1, Q5 RULED). Every row of §2.3 is pinned
//! here on the JS backend; the native backend's twins are in
//! `vilan-cli/tests/native_differential.rs`. Every one of these programs was a
//! parse error before B571 (`let a = 1 as f64;`, the paper's probe a01).

use crate::support::*;

// --- §2.3: the coercions, one row each ---------------------------------------

#[test]
fn b571_an_unsuffixed_literal_takes_the_ascribed_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun half(value: f64): f64 {
            value / 2.0
        }

        fun main() {
            let a = 5 as f64;
            print(half(a));
            print(half(3 as f64));
        }
        "#,
        "2.5\n1.5\n",
    );
}

#[test]
fn b571_an_empty_list_none_and_a_generic_result_take_their_arguments() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::SignalCell;

        fun make<T>(): List<T> {
            []
        }

        fun main() {
            let words = [] as List<str>;
            print(words.len());
            let nothing = None as Option<i32>;
            print(nothing.is_none());
            let made = make() as List<str>;
            print(made.len());
            let cell = SignalCell::new([1, 2, 3] as List<usize>);
            let total: usize = cell.get().len();
            print(total);
        }
        "#,
        "0\ntrue\n0\n3\n",
    );
}

#[test]
fn b571_a_value_ascribed_to_a_trait_object_is_erased() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Shape {
            fun area(self): i32;
        }

        struct Square {
            side: i32,
        }

        struct Strip {
            length: i32,
        }

        impl Square with Shape {
            fun area(self): i32 {
                self.side * self.side
            }
        }

        impl Strip with Shape {
            fun area(self): i32 {
                self.length
            }
        }

        fun main() {
            let one = Square { side = 3 } as dyn Shape;
            print(one.area());
            let wide = true;
            let picked = match wide {
                true => Square { side = 2 },
                false => Strip { length = 5 },
            } as dyn Shape;
            print(picked.area());
            let shapes = [Square { side = 1 } as dyn Shape, Strip { length = 7 } as dyn Shape];
            print(shapes[1].area());
        }
        "#,
        "9\n4\n7\n",
    );
}

#[test]
fn b571_the_e261_match_of_two_stages_is_ascribed_inline() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::reactive::{Source, SignalCell, Flow};

        fun main() {
            let cell = SignalCell::new(2);
            let pick = true;
            let state = match pick {
                true => Source::constant(1),
                false => cell.derive(|v| v * 10),
            } as dyn Flow<i32>;
            let _ = state;
            print("built");
        }
        "#,
    );
}

/// B161 at an ascription: a bare trait is CHECKED and the concrete type KEPT,
/// so the chain after it still reaches the concrete members.
#[test]
fn b571_a_bare_trait_is_checked_and_the_concrete_type_kept() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{SignalCell, Pipe};

        trait Named {
            fun name(self): str;
        }

        struct Tag {
            label: str,
            weight: i32,
        }

        impl Tag with Named {
            fun name(self): str {
                self.label
            }
        }

        fun main() {
            let tag = Tag { label = "t", weight = 4 } as Named;
            print(tag.weight);
            let cell = SignalCell::new(3);
            let doubled = cell.derive(|value| value * 2) as Pipe<i32>;
            print(doubled.sample());
        }
        "#,
        "4\n6\n",
    );
}

#[test]
fn b571_a_bare_trait_the_value_does_not_implement_is_refused() {
    assert_fails_with(
        r#"
        trait Named {
            fun name(self): str;
        }

        struct Plain {
            n: i32,
        }

        fun main() {
            let plain = Plain { n = 1 } as Named;
            let _ = plain;
        }
        "#,
        "does not implement trait 'Named', required by the ascription",
    );
}

#[test]
fn b571_a_named_function_and_a_tuple_variant_coerce_to_a_closure_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun measure(text: str): i32 {
            text.len().as_i32()
        }

        fun apply(f: |i32| Option<i32>, value: i32): Option<i32> {
            f(value)
        }

        fun main() {
            let typed = measure as |str| i32;
            print(typed("hello"));
            let wrap = Some as |i32| Option<i32>;
            print(wrap(3).unwrap_or(0));
            print(apply(Some as |i32| Option<i32>, 9).unwrap_or(0));
        }
        "#,
        "5\n3\n9\n",
    );
}

/// B495 at an ascription: a closure literal's parameters adopt the modes the
/// ascribed closure type names.
#[test]
fn b571_a_closure_literal_adopts_the_ascribed_parameter_modes() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun each(f: |&str| void) {
            let word = "abc";
            f(&word);
        }

        fun main() {
            each((|text| print(text.len())) as |&str| void);
        }
        "#,
        "3\n",
    );
}

#[test]
fn b571_never_yields_to_the_ascribed_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::{print, panic};

        fun main() {
            let ready = true;
            let value = ready then 4 else panic("unreachable") as i32;
            print(value);
        }
        "#,
        "4\n",
    );
}

#[test]
fn b571_a_mismatch_is_refused_as_an_annotated_binding_refuses_it() {
    assert_fails_with(
        r#"
        fun main() {
            let words = ["a"] as List<i32>;
            let _ = words;
        }
        "#,
        "Expected List<i32>, but got List<str> instead",
    );
}

// --- §2.2: not a binding — a place is copied exactly where it would be -------

/// Rule 1 copies at every binding and an ascription is not one: `let ys = xs
/// as List<i32>` copies as `let ys = xs` does, and an `own` argument copies as
/// the unascribed argument does. Planted red by keying the copy at the
/// ascription instead of its value (`peel_ascriptions` removed): `ys` aliased
/// `xs` on JS.
#[test]
fn b571_an_ascribed_place_is_copied_exactly_where_the_place_would_be() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Point {
            x: i32,
            y: i32,
        }

        fun keep(own items: List<i32>): List<i32> {
            mut kept = items;
            kept.push(99);
            kept
        }

        fun echo(items: List<i32>): List<i32> {
            items as List<i32>
        }

        fun main() {
            mut xs = [1, 2];
            mut ys = xs as List<i32>;
            ys.push(3);
            print(xs.len());
            print(ys.len());
            let kept = keep(xs as List<i32>);
            print(xs.len());
            print(kept.len());
            mut back = echo(xs);
            back.push(5);
            print(xs.len());
            let p = Point { x = 1, y = 2 };
            mut q = p as Point;
            q.x = 10;
            print(p.x);
            print((p as Point).y);
        }
        "#,
        "2\n3\n2\n3\n2\n1\n2\n",
    );
}

// --- §7.1: a value, never a place --------------------------------------------

#[test]
fn b571_an_assignment_through_an_ascription_is_refused_with_the_place() {
    assert_fails_with(
        r#"
        struct Point {
            x: i32,
        }

        fun main() {
            mut p = Point { x = 1 };
            (p as Point).x = 5;
        }
        "#,
        "an ascription is a value, not a place — `as` names the type a value has, and there is \
         nothing to write through: write the place itself, `p.x`",
    );
}

#[test]
fn b571_a_mutating_method_through_an_ascription_is_refused() {
    assert_fails_with(
        r#"
        fun main() {
            mut xs = [1];
            (xs as List<i32>).push(2);
        }
        "#,
        "an ascription is a value, not a place",
    );
}

#[test]
fn b571_a_writable_view_of_an_ascription_is_refused() {
    assert_fails_with(
        r#"
        fun main() {
            mut n = 1;
            let view = &mut (n as i32);
            let _ = view;
        }
        "#,
        "an ascription is a value, not a place",
    );
}

// --- §4 / §5 / §6: the grammar's readings, through the whole pipeline --------

#[test]
fn b571_the_precedence_rows_type_as_the_paper_reads_them() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let total = 1.5 + 2 as f64;
            print(total);
            let x = 3;
            print(-x as i32);
            print(x < 4 as i32);
            let flag = true;
            print(flag then 1 as f64 else 2.5);
            let maybe = Some(4);
            print(maybe as Option<i32> is Some(let v) && v == 4);
        }
        "#,
        "3.5\n-3\ntrue\n1\ntrue\n",
    );
}

#[test]
fn b571_a_spaced_less_than_after_an_ascribed_type_is_a_comparison() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let limit: usize = 10;
            let n: usize = 4;
            print(n as usize < limit);
            print(n as usize > 2);
            print(n as (usize) < limit);
            let words = ["a", "bb"];
            print(words as List<str> == ["a", "bb"]);
        }
        "#,
        "true\ntrue\ntrue\ntrue\n",
    );
}

#[test]
fn b571_a_spaced_generic_list_after_as_is_refused_with_the_tight_spelling() {
    assert_fails_with(
        r#"
        fun main() {
            let words = [] as List <str>;
            let _ = words;
        }
        "#,
        "`as List` then `<`: a generic list after `as` touches its type, `List<str>`",
    );
}

#[test]
fn b571_each_stage_of_a_chain_is_ascribed_and_the_chain_goes_on() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let count = ["a", "bb", "ccc"] as List<str>
                .map(|word| word.len()) as List<usize>
                .filter(|length| *length > 1) as List<usize>
                .len() as usize;
            print(count);
        }
        "#,
        "2\n",
    );
}

/// A stage that disagrees is refused AT that stage: the ascribed type flows
/// into the stage the way an annotation flows into an initializer, so the
/// closure handed to `.map` is the one told — before any later stage is
/// asked anything.
#[test]
fn b571_a_mismatched_stage_is_refused_at_that_stage() {
    assert_fails_spanning(
        r#"
        fun main() {
            let count = ["a", "bb"] as List<str>
                .map(|word| word.len()) as List<str>
                .len();
            let _ = count;
        }
        "#,
        "word.len()",
        "Expected str, but got usize instead",
    );
}

/// Q4 (RULED): `as` after a `match`/`if`/`{` brace continues the form, in a
/// value position and at a statement's head on the brace's own line.
#[test]
fn b571_an_ascription_after_a_block_like_brace_continues_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let flag = true;
            let picked = match flag {
                true => 2,
                false => 3,
            } as f64;
            print(picked);
            let chosen = if flag { 7 } else { 8 } as usize;
            print(chosen);
            let summed = 1 + { 2 } as i32;
            print(summed);
            print(match flag { true => 1, false => 2 } as i32 + 10);
        }
        "#,
        "2\n7\n3\n11\n",
    );
}

/// Q6 (RULED): `as` stays contextual — a binding and a function named `as`
/// keep their meaning (the paper's probe a07), and a name `as` on the line
/// after a block-like statement begins a new statement.
#[test]
fn b571_as_is_still_a_name_where_a_name_can_stand() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun as(value: i32): i32 {
            value + 1
        }

        fun main() {
            let flag = true;
            if flag {
                print(1);
            }
            as(2);
            print(as(4));
            let as = 5;
            print(as);
        }
        "#,
        "1\n5\n5\n",
    );
}
