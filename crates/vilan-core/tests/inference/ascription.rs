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
        "`[\"a\"]` is `List<str>`, not `List<i32>`",
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
        "an ascription is a value, never a place to view",
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

// --- S2: the refusals teach once (§7, §8) -------------------------------------

/// Q7 (RULED): `as` names a type, it does not convert — the numeric refusal
/// carries the conversion that exists, which `check --fix` writes.
#[test]
fn b571_an_ascription_to_another_width_is_refused_with_the_conversion() {
    assert_fails_spanning(
        r#"
        fun main() {
            let n: i32 = 3;
            let x = n as f64;
            let _ = x;
        }
        "#,
        "n as f64",
        "`n` is `i32`, not `f64` (ascribed here): `as` names the type a value already has, and \
         does not convert. There are no implicit numeric conversions; convert with `.as_f64()`",
    );
}

/// §8: a stage that disagrees is named, at the ascription, with the stage's
/// own link noted.
#[test]
fn b571_a_mismatched_stage_names_the_stage() {
    assert_fails_noting(
        r#"
        fun main() {
            let words = ["a", "bb"];
            let count = words.len() as i32;
            let _ = count;
        }
        "#,
        "`.len()` returns `usize`, not `i32` (ascribed here): `as` names the type a value \
         already has, and does not convert",
        "len()",
        "this stage returns `usize`",
    );
}

#[test]
fn b571_a_plain_mismatch_names_the_value_and_both_types() {
    assert_fails_with(
        r#"
        struct Foo {
            n: i32,
        }

        struct Bar {
            n: i32,
        }

        fun main() {
            let bar = Bar { n = 1 };
            let foo = bar as Foo;
            let text = "12";
            let number = text as i32;
            let _ = (foo, number);
        }
        "#,
        "`bar` is `Bar`, not `Foo`",
    );
}

#[test]
fn b571_a_string_is_not_converted_either() {
    assert_fails_with(
        r#"
        fun main() {
            let text = "12";
            let number = text as i32;
            let _ = number;
        }
        "#,
        "`text` is `str`, not `i32`",
    );
}

/// §7: nothing narrows an object.
#[test]
fn b571_a_trait_object_is_not_downcast() {
    assert_fails_with(
        r#"
        trait Shape {
            fun area(self): i32;
        }

        struct Square {
            side: i32,
        }

        impl Square with Shape {
            fun area(self): i32 {
                self.side
            }
        }

        fun main() {
            let shape: dyn Shape = Square { side = 2 };
            let square = shape as Square;
            let _ = square;
        }
        "#,
        "Expected Square, but got dyn Shape instead: an object does not narrow back to the type it \
         erased",
    );
}

/// §6's wart: `await p as T` ascribes the promise; when `T` is the awaited
/// type the refusal says so.
#[test]
fn b571_an_awaited_ascription_steers_to_the_awaited_value() {
    assert_fails_with(
        r#"
        async fun main() {
            let promise = async { 4 };
            let value = await promise as i32;
            let _ = value;
        }
        "#,
        "`await p as T` ascribes the PROMISE, because `as` binds tighter than `await` — ascribe \
         the awaited value, `(await p) as i32`",
    );
}

#[test]
fn b571_an_ascribed_awaited_value_checks() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        async fun fetch(): i32 {
            4
        }

        async fun main() {
            let value = (await fetch()) as i32;
            print(value);
        }
        "#,
        "4\n",
    );
}

/// §6: `&x as &T` reads `&(x as &T)`, a view of a value — refused with the
/// view taken first.
#[test]
fn b571_a_view_of_an_ascription_is_refused_with_the_view_first() {
    assert_fails_with(
        r#"
        fun main() {
            let n = 3;
            let view = &n as &i32;
            let _ = view;
        }
        "#,
        "`&` here takes a view of an ascription, and an ascription is a value, never a place to \
         view — `as` binds tighter than `&`: take the view first and ascribe it, `(&n) as &i32`",
    );
}

/// §8: the cannot-infer steer offers the inline spelling.
#[test]
fn b571_cannot_infer_offers_the_ascription() {
    assert_fails_with(
        r#"
        fun make<T>(): List<T> {
            []
        }

        fun main() {
            let made = make();
            let _ = made;
        }
        "#,
        "as the call's type argument (`make<…>(…)`), or ascribe the call (`make(…) as …`)",
    );
}

/// §2.3: E261's steer names the inline form.
#[test]
fn b571_the_e261_steer_names_the_ascription() {
    assert_fails_with(
        r#"
        import std::reactive::{Source, SignalCell, Flow};

        fun main() {
            let cell = SignalCell::new(2);
            let pick = true;
            let state = match pick {
                true => Source::constant(1),
                false => cell.derive(|v| v * 10),
            };
            let _ = state;
        }
        "#,
        "or ascribe the form, `match .. { .. } as dyn Flow<T>`",
    );
}

/// B569: a labelled tuple literal is matched to an ascribed labelled type by
/// NAME, as at an annotated binding — and a place whose labels contradict the
/// ascription's is refused with the label that moved and both rewrites.
#[test]
fn b571_tuple_labels_through_an_ascription_read_as_at_a_binding() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let by_name = (x = 1, y = 2) as (y: i32, x: i32);
            let bound: (y: i32, x: i32) = (x = 1, y = 2);
            print(by_name.0 == bound.0);
            print(by_name.y);
            let labelled = (3, 4) as (w: i32, h: i32);
            print(labelled.h);
        }
        "#,
        "true\n2\n4\n",
    );
    assert_fails_with(
        r#"
        fun main() {
            let p = (x = 1, y = 2);
            let q = p as (y: i32, x: i32);
            let _ = q;
        }
        "#,
        "`p` is `(x: i32, y: i32)`, not `(y: i32, x: i32)`: the label `x` names slot 0 of the \
         value and slot 1 here, so the two do not convert — match by name, writing the labels \
         out, or by position, dropping them: by name, `(y = p.y, x = p.x)`; by position, \
         `(p.0, p.1)`",
    );
}
