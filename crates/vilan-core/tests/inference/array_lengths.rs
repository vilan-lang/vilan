//! Array lengths (`proposal/array-lengths.md`): a fixed array's length is a
//! type-level number (R2), so an impl head may bind it (`[type T; _]`,
//! `[type T; const N]`, S2), a generic list may declare it (`<const N>`, S3),
//! and std implements its traits once for every length (S5).
//!
//! The native half of each feature is pinned by the native differential
//! (`crates/vilan-cli/tests/native/array_lengths*.vl`).

use crate::support::*;

// --- S2: the any-length impl head --------------------------------------------

/// The anonymous length binder: one impl answers every length, and `len()`
/// folds per instance to the length the receiver has.
#[test]
fn s2_an_anonymous_length_binder_answers_every_length() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Count {
            fun count(self): usize;
        }
        impl [type T; _] with Count {
            fun count(self): usize {
                self.len()
            }
        }
        fun main() {
            let one: [i32; 1] = [7];
            let three: [str; 3] = ["a", "b", "c"];
            let empty: [bool; 0] = [];
            print(one.count());
            print(three.count());
            print(empty.count());
            print([0; 5].count());
        }
        "#,
        "1\n3\n0\n5\n",
    );
}

/// The named spelling: `N` is the length, and reads as a `usize` value in the
/// body (§5.3), folded per instance; `[value; N]` repeats it.
#[test]
fn s2_a_named_length_binder_is_a_usize_value_in_the_body() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Shape {
            fun size(self): usize;
            fun zeroed(self): [i32; 4];
        }
        impl [type T; const N] with Shape {
            fun size(self): usize {
                N * 10 + self.len()
            }
            fun zeroed(self): [i32; 4] {
                [0; 4]
            }
        }
        fun main() {
            let pair: [i32; 2] = [1, 2];
            let triple: [str; 3] = ["x", "y", "z"];
            print(pair.size());
            print(triple.size());
            print(pair.zeroed().len());
        }
        "#,
        "22\n33\n4\n",
    );
}

/// A repeat of the binder's length inside the body is an array of the
/// instance's length.
#[test]
fn s2_a_repeat_of_the_length_parameter_has_the_instances_length() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Blank {
            fun blank_count(self): usize;
        }
        impl [type T; const N] with Blank {
            fun blank_count(self): usize {
                let blanks = [false; N];
                blanks.len()
            }
        }
        fun main() {
            let three: [i32; 3] = [1, 2, 3];
            let five: [i32; 5] = [1, 2, 3, 4, 5];
            print(three.blank_count());
            print(five.blank_count());
        }
        "#,
        "3\n5\n",
    );
}

/// The element binds beside the length, a bound on the element is checked,
/// and the body walks and indexes at element `T`.
#[test]
fn s2_the_element_binds_beside_the_length_and_keeps_its_bound() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;
        trait Joined {
            fun joined(self): str;
        }
        impl [type T: Display; _] with Joined {
            fun joined(self): str {
                mut out = "";
                for item in self {
                    out = out + item.to_string() + ";";
                }
                out + self[0].to_string()
            }
        }
        fun main() {
            let numbers: [i32; 3] = [1, 2, 3];
            let words: [str; 2] = ["a", "b"];
            print(numbers.joined());
            print(words.joined());
        }
        "#,
        "1;2;3;1\na;b;a\n",
    );
    assert_fails_with(
        r#"
        import std::display::Display;
        trait Joined {
            fun joined(self): str;
        }
        impl [type T: Display; _] with Joined {
            fun joined(self): str {
                self[0].to_string()
            }
        }
        struct Opaque {
            value: i32,
        }
        fun main() {
            let opaque: [Opaque; 1] = [Opaque { value = 1 }];
            let _joined = opaque.joined();
        }
        "#,
        "[Opaque; 1] has no method 'joined'",
    );
}

/// §5.4: a literal length outranks a length binder by the existing
/// specificity order — `[i32; 3]` takes its own impl, every other length the
/// blanket.
#[test]
fn s2_a_literal_length_outranks_the_length_binder() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Name {
            fun name(self): str;
        }
        impl [i32; 3] with Name {
            fun name(self): str {
                "three"
            }
        }
        impl [type T; _] with Name {
            fun name(self): str {
                "any"
            }
        }
        fun main() {
            let three: [i32; 3] = [1, 2, 3];
            let two: [i32; 2] = [1, 2];
            print(three.name());
            print(two.name());
        }
        "#,
        "three\nany\n",
    );
}

/// The binder nests: `Option<[type T; _]>` binds through the payload.
#[test]
fn s2_the_length_binder_nests_inside_a_generic_argument() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Width {
            fun width(self): usize;
        }
        impl Option<[type T; const N]> with Width {
            fun width(self): usize {
                N
            }
        }
        fun main() {
            let some: Option<[i32; 4]> = Some([1, 2, 3, 4]);
            print(some.width());
        }
        "#,
        "4\n",
    );
}

/// Two length binders are two parameters (B294's rule for `_`): a tuple of
/// two arrays binds each length on its own.
#[test]
fn s2_two_anonymous_length_binders_are_independent() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Sizes {
            fun sizes(self): usize;
        }
        impl ([type A; _], [type B; _]) with Sizes {
            fun sizes(self): usize {
                self.0.len() * 10 + self.1.len()
            }
        }
        fun main() {
            let pair: ([i32; 2], [str; 3]) = ([1, 2], ["a", "b", "c"]);
            print(pair.sizes());
        }
        "#,
        "23\n",
    );
}

/// Inside the body the length is RIGID: `self` is no `[T; 3]`, whatever
/// length a caller has; a whole-type parameter still takes it.
#[test]
fn s2_the_length_parameter_is_rigid_inside_the_body() {
    assert_fails_with(
        r#"
        fun three<T>(values: [T; 3]): usize {
            values.len()
        }
        trait Count {
            fun count(self): usize;
        }
        impl [type T; _] with Count {
            fun count(self): usize {
                three(self)
            }
        }
        fun main() {
            let numbers: [i32; 3] = [1, 2, 3];
            let _n = numbers.count();
        }
        "#,
        "Expected [T; 3]",
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun keep<U>(value: U): U {
            value
        }
        trait Count {
            fun count(self): usize;
        }
        impl [type T; _] with Count {
            fun count(self): usize {
                keep(self).len()
            }
        }
        fun main() {
            let numbers: [i32; 3] = [1, 2, 3];
            print(numbers.count());
        }
        "#,
        "3\n",
    );
}

/// §5.3: a destructuring needs the length, and a length parameter has none
/// to give at the point the body is checked.
#[test]
fn s2_a_destructuring_of_a_length_parameter_array_is_refused() {
    assert_fails_with(
        r#"
        trait First {
            fun first_two(self): i32;
        }
        impl [i32; _] with First {
            fun first_two(self): i32 {
                let [a, b] = self;
                a + b
            }
        }
        fun main() {
            let pair: [i32; 2] = [1, 2];
            let _n = pair.first_two();
        }
        "#,
        "a destructuring needs the array's length",
    );
}

/// A literal index into a length-parameter array is admitted (there is no
/// length to check it against when the body is checked) and keeps its
/// runtime bounds check.
#[test]
fn s2_a_literal_index_into_a_length_parameter_array_is_checked_at_runtime() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Head {
            fun head(self): i32;
        }
        impl [i32; _] with Head {
            fun head(self): i32 {
                self[0]
            }
        }
        fun main() {
            let three: [i32; 3] = [4, 5, 6];
            print(three.head());
        }
        "#,
        "4\n",
    );
    assert_run_panics(
        r#"
        trait Head {
            fun head(self): i32;
        }
        impl [i32; _] with Head {
            fun head(self): i32 {
                self[0]
            }
        }
        fun main() {
            let empty: [i32; 0] = [];
            let _h = empty.head();
        }
        main();
        "#,
        "index",
    );
}

/// The binders are introduced in an impl subject and nowhere else (B294's
/// sentence, said for lengths).
#[test]
fn s2_a_length_binder_outside_an_impl_subject_is_refused() {
    assert_fails_with(
        r#"
        fun main() {
            let values: [i32; _] = [1, 2];
        }
        "#,
        "`_` introduces a length parameter, and only an `impl` subject introduces one",
    );
    assert_fails_with(
        r#"
        fun total(values: [i32; const N]): i32 {
            values[0]
        }
        fun main() {
            print(total([1, 2]));
        }
        "#,
        "`const N` introduces a length parameter, and only an `impl` subject introduces one",
    );
}

/// §3's kind checks: a length parameter is not a type, and a type is not a
/// length.
#[test]
fn s2_the_kind_checks_keep_lengths_and_types_apart() {
    assert_fails_with(
        r#"
        trait Wrong {
            fun wrong(self): N;
        }
        impl [type T; const N] with Wrong {
            fun wrong(self): N {
                self[0]
            }
        }
        fun main() {}
        "#,
        "`N` is a length, not a type",
    );
    assert_fails_with(
        r#"
        trait Wrong {
            fun wrong(self): usize;
        }
        impl [type T; T] with Wrong {
            fun wrong(self): usize {
                0
            }
        }
        fun main() {}
        "#,
        "`T` is not a length",
    );
}

/// The formatter writes both binder spellings back as written.
#[test]
fn s2_both_binder_spellings_survive_the_formatter() {
    let source = "trait Count {\n\tfun count(self): usize;\n}\n\nimpl [type T; _] with Count {\n\tfun count(self): usize {\n\t\tself.len()\n\t}\n}\n\nimpl Option<[type T; const N]> with Count {\n\tfun count(self): usize {\n\t\tN\n\t}\n}\n";
    assert_eq!(
        vilan_core::formatter::reprint(source).as_deref(),
        Ok(source)
    );
}

/// §5.3: a `const` expression evaluates once per site, and a length
/// parameter differs per instance — reading one is refused, where it folded
/// to `0` before the refusal existed.
#[test]
fn s2_a_const_expression_may_not_read_a_length_parameter() {
    assert_fails_with(
        r#"
        trait Width {
            fun width(self): usize;
        }
        impl [i32; const N] with Width {
            fun width(self): usize {
                const N + 1
            }
        }
        fun main() {
            let three: [i32; 3] = [1, 2, 3];
            let _w = three.width();
        }
        "#,
        "`N` is a length parameter: it is fixed per instance",
    );
}

/// The inferred fold (const-eval.md §9) leaves a binding over a length
/// parameter at runtime, so each instance reads its own length.
#[test]
fn s2_a_binding_over_a_length_parameter_is_read_per_instance() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Width {
            fun width(self): usize;
        }
        impl [i32; const N] with Width {
            fun width(self): usize {
                let doubled = N * 2;
                doubled + 1
            }
        }
        fun main() {
            let three: [i32; 3] = [1, 2, 3];
            let one: [i32; 1] = [1];
            print(three.width());
            print(one.width());
        }
        "#,
        "7\n3\n",
    );
}

// --- S3: `<const N>` in generic lists ----------------------------------------

/// §5.2's first door: a length binds from an ARGUMENT, and each length is its
/// own instance; `N` reads as a `usize` in the body.
#[test]
fn s3_a_length_parameter_binds_from_an_argument() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun total<const N>(values: [i32; N]): i32 {
            mut sum = 0;
            for value in values {
                sum = sum + value;
            }
            sum
        }
        fun size<T, const N>(values: [T; N]): usize {
            N
        }
        fun main() {
            let row: [i32; 3] = [1, 2, 3];
            let pair: [i32; 2] = [10, 20];
            print(total(row));
            print(total(pair));
            print(size(["a", "b", "c", "d"]));
        }
        "#,
        "6\n30\n4\n",
    );
}

/// §5.2's second door: a DIRECTED literal binds the length by its count.
#[test]
fn s3_a_directed_literal_binds_the_length_by_its_count() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun total<const N>(values: [i32; N]): i32 {
            mut sum = 0;
            for value in values {
                sum = sum + value;
            }
            sum
        }
        fun count<const N>(values: [str; N]): usize {
            N
        }
        fun main() {
            print(total([1, 2, 3]));
            print(total([5]));
            print(count(["a", "b", "c"]));
        }
        "#,
        "6\n5\n3\n",
    );
}

/// §5.2's third and fourth doors: the EXPECTED type and an explicit numeric
/// argument; a length nothing binds is refused by B580's rule, with the
/// numeric spelling in the steer.
#[test]
fn s3_a_length_binds_from_the_expected_type_or_an_explicit_number() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun zeros<const N>(): [i32; N] {
            [0; N]
        }
        fun main() {
            let four: [i32; 4] = zeros();
            print(four.len());
            print(zeros<2>().len());
        }
        "#,
        "4\n2\n",
    );
    assert_fails_with(
        r#"
        fun zeros<const N>(): [i32; N] {
            [0; N]
        }
        fun main() {
            let unknown = zeros();
        }
        "#,
        "cannot infer the length 'N' for this call",
    );
}

/// Two uses of one `N` must agree; two parameters are independent.
#[test]
fn s3_two_uses_of_one_length_must_agree() {
    assert_fails_with(
        r#"
        fun zip_count<T, U, const N>(left: [T; N], right: [U; N]): usize {
            N
        }
        fun main() {
            let left: [i32; 2] = [1, 2];
            let right: [str; 1] = ["a"];
            let _n = zip_count(left, right);
        }
        "#,
        "Expected [U; 2], but got [str; 1]",
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun sizes<const A, const B>(left: [i32; A], right: [i32; B]): usize {
            A * 10 + B
        }
        fun main() {
            print(sizes([1, 2], [1, 2, 3]));
        }
        "#,
        "23\n",
    );
}

/// Q8: a struct generic over a length — the field's length is the
/// parameter, a literal binds it, an annotation writes it as a number.
#[test]
fn s3_a_struct_may_be_generic_over_a_length() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct Ring<T, const N> {
            slots: [T; N],
            at: usize,
        }
        impl Ring<type T, const N> {
            fun capacity(self): usize {
                N
            }
        }
        fun main() {
            let ring = Ring { slots = [1, 2, 3], at = 0 };
            print(ring.capacity());
            let written: Ring<str, 2> = Ring { slots = ["a", "b"], at = 1 };
            print(written.capacity());
            print(written.slots.len());
        }
        "#,
        "3\n2\n2\n",
    );
}

/// The kinds hold at an application: a number where a type belongs, and a
/// type where a length belongs, are both refused — at a call and at a written
/// type.
#[test]
fn s3_a_generic_argument_keeps_its_kind() {
    assert_fails_with(
        r#"
        fun zeros<const N>(): [i32; N] {
            [0; N]
        }
        fun main() {
            let _z = zeros<i32>();
        }
        "#,
        "`N` is a length parameter: give it a number",
    );
    assert_fails_with(
        r#"
        import std::list::List;
        fun main() {
            let _xs: List<4> = [];
        }
        "#,
        "`4` is a length, and `T` takes a type",
    );
    assert_fails_with(
        r#"
        struct Ring<T, const N> {
            slots: [T; N],
        }
        fun main() {
            let _ring: Ring<i32, str> = Ring { slots = [1] };
        }
        "#,
        "`N` is a length parameter: give it a number",
    );
}

/// The formatter writes `<const N>` and a numeric argument back.
#[test]
fn s3_const_parameters_and_numeric_arguments_survive_the_formatter() {
    let source = "fun zeros<const N>(): [i32; N] {\n\t[0; N]\n}\n\nstruct Ring<T, const N> {\n\tslots: [T; N],\n}\n\nfun main() {\n\tlet _z = zeros<4>();\n\tlet _r: Ring<i32, 2> = Ring { slots = [1, 2] };\n}\n";
    assert_eq!(
        vilan_core::formatter::reprint(source).as_deref(),
        Ok(source)
    );
}

/// An impl head over a length-generic struct binds the length with
/// `const N` (or `_`); a TYPE binder at that position is refused.
#[test]
fn s3_an_impl_head_binds_a_structs_length_with_const() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct Ring<T, const N> {
            slots: [T; N],
        }
        trait Size {
            fun size(self): usize;
        }
        impl Ring<type T, _> with Size {
            fun size(self): usize {
                self.slots.len()
            }
        }
        fun main() {
            print(Ring { slots = [true, false] }.size());
        }
        "#,
        "2\n",
    );
    assert_fails_with(
        r#"
        struct Ring<T, const N> {
            slots: [T; N],
        }
        impl Ring<type T, type N> {
            fun size(self): usize {
                0
            }
        }
        fun main() {}
        "#,
        "this position is a length parameter, so its binder is `const N`, not `type N`",
    );
}

/// A length parameter passes on as a generic ARGUMENT (`Slots<N>`), where the
/// parameter's kind admits it; an enum may be generic over a length.
#[test]
fn s3_a_length_parameter_passes_on_as_a_generic_argument() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        enum Slots<const N> {
            Full([i32; N]),
            Empty,
        }
        fun filled<const N>(slots: Slots<N>): usize {
            match slots {
                Slots::Full(let values) => values.len() + N,
                Slots::Empty => 0,
            }
        }
        fun main() {
            let full: Slots<3> = Slots::Full([4, 5, 6]);
            let empty: Slots<2> = Slots::Empty;
            print(filled(full));
            print(filled(empty));
        }
        "#,
        "6\n0\n",
    );
}

/// A trait may be generic over a length, and an impl provides it at a number.
#[test]
fn s3_a_trait_may_be_generic_over_a_length() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Fill<const N> {
            fun fill(self): [i32; N];
        }
        struct Seed {
            value: i32,
        }
        impl Seed with Fill<3> {
            fun fill(self): [i32; 3] {
                [self.value; 3]
            }
        }
        fun main() {
            let seed = Seed { value = 7 };
            print(seed.fill().len());
            print(seed.fill()[2]);
        }
        "#,
        "3\n7\n",
    );
}

/// A directed literal at an OPEN element types by its items, as it takes its
/// own count at an open length — the call grounds `T` from it (natively the
/// element went unbound before; JS never asked). The native half is the
/// differential's `array_length_generics.vl`.
#[test]
fn a_directed_literal_at_an_open_element_types_by_its_items() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun first<T>(values: [T; 3]): T {
            values[0]
        }
        fun count<T, const N>(values: [T; N]): usize {
            N
        }
        struct Trio<T> {
            slots: [T; 3],
        }
        fun main() {
            print(first(["a", "b", "c"]));
            print(count([true, false]));
            let trio = Trio { slots = [7, 8, 9] };
            print(trio.slots[1]);
        }
        "#,
        "a\n2\n8\n",
    );
}
