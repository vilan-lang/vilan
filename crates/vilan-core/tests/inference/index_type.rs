//! I5 S1: `usize`, the index type (`proposal/index-type.md`).
//!
//! A DISTINCT numeric type — `u53` on the JS targets, the platform word
//! natively — with the whole sized family's surface (§2.3), a `42usize`
//! suffix (§11 Q5), the JS range guarantee on every backend (§11 Q3), a `Wire`
//! impl at `i32`'s width (§6), and the subscript admitting it beside `i32`
//! until S2 moves std's signatures. No std signature moves in S1.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- the type -----------------------------------------------------------------

#[test]
fn usize_is_a_distinct_type_not_an_alias_of_u53() {
    // Ruling 1: distinct. A `usize` does not flow into a `u53` (or back)
    // without the conversion E218's steer names.
    assert_fails_with(
        concat!(
            "fun main() {\n",
            "\tlet at: usize = 3usize;\n",
            "\tlet id: u53 = at;\n",
            "\tprint(i\"{id}\");\n",
            "}\n",
        ),
        "Expected u53, but got usize instead. There are no implicit numeric conversions; \
         convert with `.as_u53()`",
    );
}

#[test]
fn an_i32_does_not_flow_into_a_usize() {
    assert_fails_with(
        concat!(
            "fun main() {\n",
            "\tlet xs = [1, 2];\n",
            "\tlet n: usize = xs.len();\n",
            "\tprint(i\"{n}\");\n",
            "}\n",
        ),
        "Expected usize, but got i32 instead. There are no implicit numeric conversions; \
         convert with `.as_usize()`",
    );
}

#[test]
fn an_unsuffixed_literal_takes_usize_from_an_annotation() {
    assert_compiles_and_runs(
        "fun main() { let n: usize = 3; let m: usize = n + 4; print(i\"{m}\"); }",
        "7\n",
    );
}

#[test]
fn the_usize_suffix_names_the_type() {
    // §11 Q5: it exists, and follows from `NUMERIC_SUFFIXES`.
    assert_compiles_and_runs(
        "fun main() { let n = 42usize; let m: usize = n; print(i\"{m}\"); }",
        "42\n",
    );
}

#[test]
fn a_usize_literal_past_the_js_guarantee_is_out_of_range() {
    assert_fails_with(
        "fun main() { let n = 9007199254740993usize; print(i\"{n}\"); }",
        "the literal `9007199254740993` is out of range for `usize` (exact integers span ±2^53 \
         on the JS backend; use `BigInt` for larger values)",
    );
}

#[test]
fn the_top_of_the_js_guarantee_is_a_usize_literal() {
    assert_compiles_and_runs(
        "fun main() { let n = 9007199254740992usize; print(i\"{n}\"); }",
        "9007199254740992\n",
    );
}

// --- B407: a NEGATIVE literal at an UNSIGNED type is refused -----------------
//
// `let n: usize = -1;` (and `u53`, `u8`) compiled and printed `-1`: the literal
// range check read the literal UNDER the minus, `1`, which fits. With B389's
// literal law every `-1` sentinel of index-type.md §3.4 would compile silently
// as a `usize` after I5 S2 — the class the migration most needs to catch. The
// seven sentinel shapes are pinned at `usize` below (spelled over user
// functions: std's signatures move in S2, not here).

fn b407_refused(program: &str, type_name: &str) {
    assert_fails_with(program, &format!("`{type_name}` is unsigned"));
    assert_fails_with(program, "so the negative literal `-1` is out of range");
}

#[test]
fn b407_a_negative_literal_is_refused_at_an_annotated_usize() {
    b407_refused(
        "fun main() {\n\tlet n: usize = -1;\n\tprint(i\"{n}\");\n}\n",
        "usize",
    );
}

#[test]
fn b407_a_negative_literal_is_refused_at_an_annotated_u53() {
    b407_refused(
        "fun main() {\n\tlet n: u53 = -1;\n\tprint(i\"{n}\");\n}\n",
        "u53",
    );
}

#[test]
fn b407_a_negative_literal_is_refused_at_an_annotated_u8() {
    b407_refused(
        "fun main() {\n\tlet n: u8 = -1;\n\tprint(i\"{n}\");\n}\n",
        "u8",
    );
}

/// The range is named, per width.
#[test]
fn b407_the_refusal_names_the_types_range() {
    assert_fails_with(
        "fun main() {\n\tlet n: u8 = -1;\n\tprint(i\"{n}\");\n}\n",
        "`u8` is unsigned (0 ..= 255)",
    );
    assert_fails_with(
        "fun main() {\n\tlet n: u32 = -7;\n\tprint(i\"{n}\");\n}\n",
        "`u32` is unsigned (0 ..= 4294967295), so the negative literal `-7` is out of range",
    );
    assert_fails_with(
        "fun main() {\n\tlet n: usize = -1;\n\tprint(i\"{n}\");\n}\n",
        "`usize` is unsigned (0 ..= 2^53 on the JS backend)",
    );
}

/// A SUFFIX names the unsigned type as surely as an annotation does.
#[test]
fn b407_a_negative_suffixed_unsigned_literal_is_refused() {
    b407_refused(
        "fun main() {\n\tlet n = -1usize;\n\tprint(i\"{n}\");\n}\n",
        "usize",
    );
}

/// Sentinel S1 (`reconcile`'s `next_same.push(-1)`): a push into a
/// `List<usize>` — the literal typed through the method's generic argument.
#[test]
fn b407_sentinel_a_negative_pushed_into_a_list_of_usize_is_refused() {
    b407_refused(
        concat!(
            "fun main() {\n",
            "\tlet mut next_same: List<usize> = [];\n",
            "\tnext_same.push(-1);\n",
            "\tprint(i\"{next_same.len()}\");\n",
            "}\n",
        ),
        "usize",
    );
}

/// Sentinel S2 (`None => -1`): a `match` arm whose value is a `usize`.
#[test]
fn b407_sentinel_a_negative_match_arm_at_usize_is_refused() {
    b407_refused(
        concat!(
            "fun found(): Option<usize> { None }\n",
            "fun main() {\n",
            "\tlet at: usize = match found() {\n",
            "\t\tSome(let index) => index,\n",
            "\t\tNone => -1,\n",
            "\t};\n",
            "\tprint(i\"{at}\");\n",
            "}\n",
        ),
        "usize",
    );
}

/// Sentinels S3 and S5 (`mut candidate = -1` / `mut last_applied = -1`, later
/// assigned an index and read AS one — db.vl's `migrations[last_applied]`):
/// B389's bare binding takes `usize` from that typed use.
#[test]
fn b407_sentinel_a_negative_binding_assigned_a_usize_later_is_refused() {
    b407_refused(
        concat!(
            "fun name_at(names: List<str>, at: usize): str { names[at] }\n",
            "fun main() {\n",
            "\tlet names = [\"a\", \"b\", \"c\"];\n",
            "\tlet index: usize = 2usize;\n",
            "\tmut last_applied = -1;\n",
            "\tif names.len() > 0 { last_applied = index; }\n",
            "\tprint(name_at(names, last_applied));\n",
            "}\n",
        ),
        "usize",
    );
}

/// Sentinel S4 (`mut highest = -1` in `settled_steps`, then compared against
/// and assigned a step): typed by the comparison.
#[test]
fn b407_sentinel_a_negative_binding_compared_with_a_usize_is_refused() {
    b407_refused(
        concat!(
            "fun main() {\n",
            "\tlet step: usize = 3usize;\n",
            "\tmut highest = -1;\n",
            "\tif step > highest { highest = step; }\n",
            "\tprint(i\"{highest}\");\n",
            "}\n",
        ),
        "usize",
    );
}

/// Sentinels S6 and S7 (the corpus's `index_of(..).unwrap_or(-1)`): the
/// literal typed through `Option<usize>::unwrap_or`'s argument.
#[test]
fn b407_sentinel_a_negative_unwrap_or_default_at_usize_is_refused() {
    b407_refused(
        concat!(
            "fun position(xs: List<i32>, wanted: i32): Option<usize> {\n",
            "\tmut at = 0usize;\n",
            "\tfor x in xs {\n",
            "\t\tif x == wanted { ret Some(at); }\n",
            "\t\tat += 1;\n",
            "\t}\n",
            "\tNone\n",
            "}\n",
            "fun main() {\n",
            "\tlet xs = [10, 20, 30];\n",
            "\tprint(i\"{position(xs, 20).unwrap_or(-1)}\");\n",
            "}\n",
        ),
        "usize",
    );
}

/// The controls: a negative literal at a SIGNED type, the signed minimum
/// written as a minus over the literal, and zero at an unsigned type.
#[test]
fn b407_negative_literals_at_signed_types_and_zero_stay_accepted() {
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet a: i32 = -1;\n",
            "\tlet b = -128i8;\n",
            "\tlet c: i53 = -5;\n",
            "\tlet d: usize = 0;\n",
            "\tprint(i\"{a} {b} {c} {d}\");\n",
            "}\n",
        ),
        "-1 -128 -5 0\n",
    );
}

/// B389's `literal_types` now reaches the range check too: a literal typed by
/// its context alone (B406's field-typed constructor argument) is checked at
/// that width rather than skipped.
#[test]
fn b407_a_context_typed_literal_is_range_checked_at_its_width() {
    assert_fails_with(
        concat!(
            "import std::shared::Shared;\n",
            "struct Counter { count: Shared<u8> }\n",
            "fun main() { let c = Counter { count = Shared::new(300) }; }\n",
        ),
        "the literal `300` is out of range for `u8`",
    );
}

#[test]
fn usize_bounds_are_the_js_guarantee() {
    // §11 Q3: `max_value()` answers 2^53 on every backend, as R6 of Order 37
    // settled `i53`/`u53` — the value is portable.
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet top = usize::max_value();\n",
            "\tlet bottom = usize::min_value();\n",
            "\tprint(i\"{top} {bottom}\");\n",
            "}\n",
        ),
        "9007199254740992 0\n",
    );
}

// --- the family (§2.3) ----------------------------------------------------------

#[test]
fn usize_arithmetic_truncates_and_orders() {
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet a: usize = 42;\n",
            "\tlet b: usize = 5;\n",
            "\tlet sum = a + b;\n",
            "\tlet difference = a - b;\n",
            "\tlet product = a * b;\n",
            "\tlet quotient = a / b;\n",
            "\tlet remainder = a.rem(b);\n",
            "\tprint(i\"{sum} {difference} {product} {quotient} {remainder}\");\n",
            "\tprint(i\"{a < b} {a > b} {a == 42} {a != b}\");\n",
            "\tlet low = a.min(b);\n",
            "\tlet high = a.max(b);\n",
            "\tlet squared = b.pow(2);\n",
            "\tprint(i\"{low} {high} {squared}\");\n",
            "}\n",
        ),
        "47 37 210 8 2\nfalse true true true\n5 42 25\n",
    );
}

#[test]
fn checked_sub_answers_none_below_zero() {
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet a: usize = 3;\n",
            "\tlet b: usize = 5;\n",
            "\tprint(a.checked_sub(b).is_none());\n",
            "\tlet back: usize = b.checked_sub(a).unwrap_or(0usize);\n",
            "\tprint(i\"{back}\");\n",
            "\tlet same: usize = a.checked_sub(a).unwrap_or(9usize);\n",
            "\tprint(i\"{same}\");\n",
            "}\n",
        ),
        "true\n2\n0\n",
    );
}

#[test]
fn saturating_sub_stops_at_zero() {
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet a: usize = 3;\n",
            "\tlet b: usize = 5;\n",
            "\tlet floor = a.saturating_sub(b);\n",
            "\tlet plain = b.saturating_sub(a);\n",
            "\tprint(i\"{floor} {plain}\");\n",
            "}\n",
        ),
        "0 2\n",
    );
}

#[test]
fn every_numeric_type_converts_to_usize() {
    // `as_usize` on the eleven other numeric types (§2.3).
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet from_i8 = 7i8.as_usize();\n",
            "\tlet from_u8 = 200u8.as_usize();\n",
            "\tlet from_i16 = 300i16.as_usize();\n",
            "\tlet from_u16 = 65535u16.as_usize();\n",
            "\tlet from_i32 = 12.as_usize();\n",
            "\tlet from_u32 = 4000000000u32.as_usize();\n",
            "\tlet from_i53 = 9007199254740000i53.as_usize();\n",
            "\tlet from_u53 = 9007199254740992u53.as_usize();\n",
            "\tlet from_f32 = 2.5f32.as_usize();\n",
            "\tlet from_f64 = 7.9f.as_usize();\n",
            "\tlet from_bigint = 3n.as_usize();\n",
            "\tprint(i\"{from_i8} {from_u8} {from_i16} {from_u16} {from_i32} {from_u32}\");\n",
            "\tprint(i\"{from_i53} {from_u53} {from_f32} {from_f64} {from_bigint}\");\n",
            "}\n",
        ),
        "7 200 300 65535 12 4000000000\n9007199254740000 9007199254740992 2 7 3\n",
    );
}

#[test]
fn usize_converts_back_to_every_width() {
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet n: usize = 300;\n",
            "\tlet to_i8 = n.as_i8();\n",
            "\tlet to_u8 = n.as_u8();\n",
            "\tlet to_i16 = n.as_i16();\n",
            "\tlet to_u16 = n.as_u16();\n",
            "\tlet to_i32 = n.as_i32();\n",
            "\tlet to_u32 = n.as_u32();\n",
            "\tlet to_i53 = n.as_i53();\n",
            "\tlet to_u53 = n.as_u53();\n",
            "\tlet to_usize = n.as_usize();\n",
            "\tlet to_f32 = n.as_f32();\n",
            "\tlet to_f64 = n.as_f64();\n",
            "\tprint(i\"{to_i8} {to_u8} {to_i16} {to_u16} {to_i32} {to_u32}\");\n",
            "\tprint(i\"{to_i53} {to_u53} {to_usize} {to_f32} {to_f64}\");\n",
            "}\n",
        ),
        "44 44 300 300 300 300\n300 300 300 300 300\n",
    );
}

#[test]
fn usize_satisfies_each_family_bound() {
    // Every trait the family owes (§2.3), each reached THROUGH a bound — the
    // way A126 found `Display` missing while interpolation worked.
    assert_compiles_and_runs(
        concat!(
            "import std::{ display::Display, debug::Debug, hash::Hashable, default::Default };\n",
            "import std::compare::{ Ord, Ordering };\n",
            "import std::json::{ Json, FromJson };\n",
            "import std::operators::{ Add, Sub, Mul, Div };\n",
            "\n",
            "fun shown<T: Display>(value: T): str { value.to_string() }\n",
            "fun debugged<T: Debug>(value: T): str { value.debug() }\n",
            "fun zero<T: Default>(): T { T::default() }\n",
            "fun larger<T: Ord>(a: T, b: T): T { if a.compare(b) == Ordering::Greater { a } else { b } }\n",
            "fun hashed<T: Hashable>(value: T): bool { value.hash() == value.hash() }\n",
            "fun encoded<T: Json>(value: T): str { value.to_json() }\n",
            "fun arithmetic<T: Add + Sub + Mul + Div>(a: T, b: T): T { (a + b) * b / b - b }\n",
            "\n",
            "fun main() {\n",
            "\tlet n: usize = 12;\n",
            "\tlet m: usize = 30;\n",
            "\tprint(shown(n));\n",
            "\tprint(debugged(n));\n",
            "\tlet nothing: usize = zero();\n",
            "\tprint(shown(nothing));\n",
            "\tprint(shown(larger(n, m)));\n",
            "\tprint(hashed(n));\n",
            "\tprint(encoded(m));\n",
            "\tprint(shown(arithmetic(n, m)));\n",
            "\tmatch usize::from_json(\"17\") {\n",
            "\t\tOk(let back) => print(shown(back)),\n",
            "\t\tErr(let reason) => print(reason),\n",
            "\t}\n",
            "}\n",
        ),
        "12\n12\n0\n30\ntrue\n30\n12\n17\n",
    );
}

#[test]
fn a_usize_keys_a_map_and_a_set() {
    assert_compiles_and_runs(
        concat!(
            "import std::{ map::Map, set::Set };\n",
            "\n",
            "fun main() {\n",
            "\tmut rows: Map<usize, str> = Map::new();\n",
            "\trows.insert(2usize, \"two\");\n",
            "\trows.insert(5usize, \"five\");\n",
            "\tprint(rows.get(5usize).unwrap_or(\"none\"));\n",
            "\tmut seen: Set<usize> = Set::new();\n",
            "\tseen.insert(3usize);\n",
            "\tseen.insert(3usize);\n",
            "\tprint(seen.len());\n",
            "}\n",
        ),
        "five\n1\n",
    );
}

// --- the wire (§6) ----------------------------------------------------------------

#[test]
fn a_usize_rides_the_wire_at_i32s_width_on_both_codecs() {
    // Ruling 5: a length or position crossing rpc KEEPS its wire width. A
    // `usize` list encodes to exactly the bytes the same `i32` list does, under
    // the binary codec (where `u53`'s lane would write eight bytes per value,
    // not four) and under the JSON codec, and both read back.
    assert_compiles_and_runs(
        concat!(
            "import std::binary::{ encode_binary, decode_binary };\n",
            "import std::json::{ encode_json, decode_json };\n",
            "import std::bytes::Bytes;\n",
            "\n",
            "fun hex_of(bytes: Bytes): str {\n",
            "\tlet digits = \"0123456789abcdef\";\n",
            "\tmut out = \"\";\n",
            "\tmut index = 0;\n",
            "\tfor index < bytes.len() {\n",
            "\t\tlet byte = bytes.get(index);\n",
            "\t\tout = out + digits.substring(byte / 16, byte / 16 + 1);\n",
            "\t\tout = out + digits.substring(byte % 16, byte % 16 + 1);\n",
            "\t\tindex = index + 1;\n",
            "\t}\n",
            "\tout\n",
            "}\n",
            "\n",
            "fun main() {\n",
            "\tlet positions: List<usize> = [0usize, 3usize, 2147483647usize];\n",
            "\tlet control: List<i32> = [0, 3, 2147483647];\n",
            "\tlet wide: List<u53> = [0u53, 3u53, 2147483647u53];\n",
            "\tlet binary = hex_of(encode_binary(positions));\n",
            "\tprint(binary == hex_of(encode_binary(control)));\n",
            "\tprint(binary == hex_of(encode_binary(wide)));\n",
            "\tprint(binary);\n",
            "\tlet text = encode_json(positions);\n",
            "\tprint(text == encode_json(control));\n",
            "\tprint(text);\n",
            "\tmatch decode_binary<List<usize>>(encode_binary(positions)) {\n",
            "\t\tOk(let back) => print(i\"{back.len()} {back[2]}\"),\n",
            "\t\tErr(let reason) => print(reason),\n",
            "\t}\n",
            "\tmatch decode_json<List<usize>>(text) {\n",
            "\t\tOk(let back) => print(i\"{back.len()} {back[1]}\"),\n",
            "\t\tErr(let reason) => print(reason),\n",
            "\t}\n",
            "}\n",
        ),
        "true\nfalse\n030000000000000003000000ffffff7f\ntrue\n[0,3,2147483647]\n3 2147483647\n3 3\n",
    );
}

#[test]
fn a_derived_wire_struct_carries_a_usize_field() {
    // `WIRE_SCALAR_NAMES` admits it, so a `[derive(Wire)]` type may hold one.
    assert_compiles_and_runs(
        concat!(
            "import std::binary::{ encode_binary, decode_binary };\n",
            "import std::wire::Wire;\n",
            "\n",
            "[derive(Wire)]\n",
            "struct Cursor { at: usize, label: str }\n",
            "\n",
            "fun main() {\n",
            "\tlet cursor = Cursor { at = 7usize, label = \"row\" };\n",
            "\tmatch decode_binary<Cursor>(encode_binary(cursor)) {\n",
            "\t\tOk(let back) => print(i\"{back.at} {back.label}\"),\n",
            "\t\tErr(let reason) => print(reason),\n",
            "\t}\n",
            "}\n",
        ),
        "7 row\n",
    );
}

// --- the subscript (B386's check, S1's admission) ----------------------------------

#[test]
fn a_usize_index_reads_a_list_an_array_and_a_write() {
    // S1 admits `usize` BESIDE `i32` at the subscript; S2 deletes the `i32`
    // half when std's signatures move.
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tmut xs: List<str> = [\"a\", \"b\", \"c\"];\n",
            "\tlet at: usize = 1;\n",
            "\tprint(xs[at]);\n",
            "\txs[at] = \"z\";\n",
            "\tprint(xs[1]);\n",
            "\tlet fixed: [i32; 3] = [4, 5, 6];\n",
            "\tlet last: usize = 2;\n",
            "\tprint(fixed[last]);\n",
            "}\n",
        ),
        "b\nz\n6\n",
    );
}

#[test]
fn a_u53_index_is_still_refused() {
    // The admission is exactly two types, not "any unsigned integer".
    assert_fails_with(
        concat!(
            "fun main() {\n",
            "\tlet xs: List<str> = [\"a\", \"b\"];\n",
            "\tlet at: u53 = 1u53;\n",
            "\tprint(xs[at]);\n",
            "}\n",
        ),
        "an index must be an `i32`, and this one is `u53`",
    );
}

#[test]
fn a_usize_index_emits_the_same_subscript_as_an_i32_one() {
    // §5.4: the JS emission does not change — a number is a number.
    let with_usize = compile(concat!(
        "fun main() {\n",
        "\tlet xs: List<str> = [\"a\", \"b\"];\n",
        "\tlet at: usize = 1;\n",
        "\tprint(xs[at]);\n",
        "}\n",
    ))
    .expect("the usize program compiles");
    let with_i32 = compile(concat!(
        "fun main() {\n",
        "\tlet xs: List<str> = [\"a\", \"b\"];\n",
        "\tlet at: i32 = 1;\n",
        "\tprint(xs[at]);\n",
        "}\n",
    ))
    .expect("the i32 program compiles");
    assert_eq!(with_usize, with_i32);
}

// --- underflow (ruling 2, §5) --------------------------------------------------------

#[test]
fn a_usize_subtracted_past_zero_goes_negative_on_js() {
    // UNSPECIFIED, never memory-unsafe: on JS the value leaves the range and
    // nothing traps; the bounds check still stands between it and a list. The
    // native backend panics in a debug build, which is why the corpus program
    // pinning this is named OUT of the native differential.
    assert_compiles_and_runs(
        concat!(
            "fun main() {\n",
            "\tlet n: usize = 0;\n",
            "\tlet under = n - 1;\n",
            "\tprint(i\"{under}\");\n",
            "}\n",
        ),
        "-1\n",
    );
}
