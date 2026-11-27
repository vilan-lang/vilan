//! Tuple keys, slice S1c of B711 (`proposal/struct-keys.md` R10): a
//! `TupleKey<T, U>` gains identity (`PartialEq`, `Hashable`, `index()`, a
//! `HashMap` key) and in-place access (`t[k]` as a place), on the JS backend.
//! The native twin of each shape is `vilan-cli/tests/native/tuple_keys_*.vl`.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

const IMPORTS: &str = "import std::hash_map::HashMap;\nimport std::tuple::{ Tuple, TupleKey };\n";

fn tuple_keys_run(body: &str, expected_stdout: &str) {
    assert_compiles_and_runs(&format!("{IMPORTS}\n{body}"), expected_stdout);
}

fn tuple_keys_refused(body: &str, message_part: &str) {
    assert_fails_with(&format!("{IMPORTS}\n{body}"), message_part);
}

// --- identity -----------------------------------------------------------------

/// A key reads its position, compares by it, hashes by it and keys a map.
#[test]
fn tuple_keys_compare_hash_and_key_a_map_by_position() {
    tuple_keys_run(
        r#"
        fun main() {
            let (a, b) = (1, 2).keys();
            print((a.index(), b.index()));
            print((a == a, a == b, a != b));
            print(a.hash() == a.hash());
            mut names: HashMap<TupleKey<(i32, i32), i32>, str> = HashMap::new();
            names.insert(a, "first");
            names.insert(b, "second");
            names.insert(a, "again");
            print((names.len(), names.get(a), names.get(b)));
            print(b.debug());
        }
        "#,
        "(0, 1)\n(true, false, true)\ntrue\n(2, Some(\"again\"), Some(\"second\"))\nTupleKey(1)\n",
    );
}

/// Two keys at different element types are different types: no `==` between them.
#[test]
fn tuple_keys_of_two_element_types_do_not_compare() {
    tuple_keys_refused(
        r#"
        fun main() {
            let (a, b) = (1, "x").keys();
            print(a == b);
        }
        "#,
        "but the right operand is `TupleKey<(i32, str), str>`",
    );
}

/// A key stays unconstructible: `TupleKey` is opaque, and no literal builds one.
#[test]
fn tuple_keys_have_no_literal() {
    tuple_keys_refused(
        "fun main() { let k: TupleKey<(i32, i32), i32> = TupleKey { }; }",
        "`TupleKey` is an `external` struct, which no literal can build: a tuple key is \
         minted only by `keys()` and `entries()`",
    );
}

// --- in-place access ------------------------------------------------------------

/// `t[k]` reads, assigns, compound-assigns, writes a member of a struct
/// element, and lends `&t[k]` / `&mut t[k]`.
#[test]
fn tuple_keys_index_a_tuple_in_place() {
    tuple_keys_run(
        r#"
        struct P { x: i32, y: i32 }

        fun bump(n: &mut i32) {
            n += 100;
        }

        fun show(n: &i32): str {
            i"<{*n}>"
        }

        fun main() {
            mut t = (1, "a", P { x = 1, y = 2 });
            let (a, b, c) = t.keys();
            print((t[a], t[b]));
            t[a] = 5;
            t[a] += 2;
            t[b] = t[b] + "z";
            t[c].x = 40;
            bump(&mut t[a]);
            print(show(&t[a]));
            print(t);
        }
        "#,
        "(1, \"a\")\n<107>\n(107, \"az\", P { x = 40, y = 2 })\n",
    );
}

/// The generic accessor: `t[k]` handed back from a view return, at a scalar
/// and at a struct element, writable and read-only.
#[test]
fn tuple_keys_the_generic_accessor_is_a_view_return() {
    tuple_keys_run(
        r#"
        struct P { x: i32, y: i32 }

        fun at<T: (2..), U>(t: &mut T, k: TupleKey<T, U>): &mut U borrows t {
            t[k]
        }

        fun peek<T: (2..), U>(t: &T, k: TupleKey<T, U>): &U borrows t {
            t[k]
        }

        fun main() {
            mut t = (1, "a", P { x = 1, y = 2 });
            let (a, b, c) = t.keys();
            at(&mut t, a) += 1;
            at(&mut t, c).y = 9;
            print(peek(&t, b));
            print(t);
        }
        "#,
        "a\n(2, \"a\", P { x = 1, y = 9 })\n",
    );
}

/// A tuple-bounded parameter walks its own positions, and a key of the family
/// indexes a tuple MAPPED from it at the mapped element type (A122 §4.3).
#[test]
fn tuple_keys_index_a_bounded_parameter_and_a_mapped_tuple() {
    tuple_keys_run(
        r#"
        import std::debug::Debug;

        fun reset<T: (2..: Debug)>(t: &mut T, from: T) {
            for k in t.keys() {
                t[k] = from[k];
            }
        }

        fun wrapped<T: (2..: Debug)>(t: T): str {
            let boxes = (x in t => Some(x));
            mut out = "";
            for k in t.keys() {
                out = out + boxes[k].debug() + ";";
            }
            out
        }

        fun main() {
            mut t = (1, 2.5, "s");
            reset(&mut t, (9, 1.5, "r"));
            print(t);
            print(wrapped((3, "x", true)));
        }
        "#,
        "(9, 1.5, \"r\")\nSome(3);Some(\"x\");Some(true);\n",
    );
}

/// An element that is itself a tuple is read and written WHOLE, its slots at
/// the position's own flat offset.
#[test]
fn tuple_keys_read_and_write_a_nested_tuple_whole() {
    tuple_keys_run(
        r#"
        fun main() {
            mut n = (1, (2, 3), 4);
            let (n0, n1, n2) = n.keys();
            print((n[n1], n[n2]));
            n[n1] = (7, 8);
            n[n2] = 9;
            n[n0] += 1;
            print(n);
        }
        "#,
        "((2, 3), 4)\n(2, (7, 8), 9)\n",
    );
}

/// K1-K3 for tuple keys: a write is what a read sees, a self-assignment
/// changes nothing, and equal keys reach the same place.
#[test]
fn tuple_keys_keep_the_laws_k1_to_k3() {
    tuple_keys_run(
        r#"
        fun same<T: (2..), U>(t: &mut T, k: TupleKey<T, U>) {
            t[k] = t[k];
        }

        fun main() {
            mut t = (1, "a");
            let (a, b) = t.keys();
            t[b] = "w";
            print(t[b] == "w");
            same(&mut t, a);
            same(&mut t, b);
            print(t);
            let again = t.keys().0;
            t[again] = 30;
            print((a == again, t[a]));
        }
        "#,
        "true\n(1, \"w\")\n(true, 30)\n",
    );
}

// --- refused ------------------------------------------------------------------

/// A key indexes only its own family (or one mapped from it).
#[test]
fn tuple_keys_index_only_their_family() {
    tuple_keys_refused(
        r#"
        fun main() {
            let (a, b) = (1, 2).keys();
            let u = (1, "x");
            print(u[a]);
        }
        "#,
        "a tuple key indexes only a tuple of the family it was minted for, or a tuple mapped \
         from that family",
    );
}

/// A nested tuple reached through a key is no place: tuples store flat.
#[test]
fn tuple_keys_refuse_a_view_of_a_nested_tuple() {
    let shared = r#"
        fun bump(p: &mut (i32, i32)) {
            p.0 += 1;
        }
    "#;
    for (body, what) in [
        ("bump(&mut n[n1]);", "a writable view of it is refused"),
        ("let v = &n[n1];", "a view of it is refused"),
        ("n[n1].0 = 5;", "a write into it is refused"),
    ] {
        tuple_keys_refused(
            &format!(
                "{shared}\nfun main() {{\n\tmut n = (1, (2, 3));\n\tlet (n0, n1) = n.keys();\n\t{body}\n}}\n"
            ),
            what,
        );
    }
    tuple_keys_refused(
        r#"
        fun at(t: &mut (i32, (i32, i32)), k: TupleKey<(i32, (i32, i32)), (i32, i32)>): &mut (i32, i32) borrows t {
            t[k]
        }
        fun main() { }
        "#,
        "this tuple key names an element that is itself a tuple, `(i32, i32)`",
    );
}
