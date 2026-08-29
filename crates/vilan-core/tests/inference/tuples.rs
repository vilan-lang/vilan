//! Tuples, fixed-length arrays, spread parameters and tuple-value spread
//! (B70/B71), and async polymorphism: `sync`, adaptation, `Task<T>`,
//! nurseries and cancellation.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- H.1: struct literals as operator operands ----------------------------------
// The operator/postfix chain admits struct literals as operands in ordinary
// expression positions; condition positions (`if`/`for` conditions, `for .. in`
// iterables, `match` subjects) exclude them so `if Foo { .. }` keeps the brace
// for the block. Parenthesize a literal to use it in a condition.

#[test]
fn a_struct_literal_is_a_left_operand() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        [derive(PartialEq)]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            let p = Point { x = 1, y = 2 };
            print(Point { x = 1, y = 2 } == p);
        }
        "#,
        "true\n",
    );
}

#[test]
fn a_struct_literal_is_a_right_operand() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        [derive(PartialEq)]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            let p = Point { x = 1, y = 2 };
            print(p != Point { x = 3, y = 4 });
        }
        "#,
        "true\n",
    );
}

#[test]
fn a_struct_literal_folds_a_field_access() {
    // The old dedicated literal member-fold, now the general postfix chain.
    assert_compiles_and_runs(
        r#"
        import std::print;

        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            print(Point { x = 3, y = 4 }.x);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_struct_literal_folds_a_method_call() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        struct Point {
            x: i32,
            y: i32,
        }

        impl Point {
            fun sum(self): i32 {
                self.x + self.y
            }
        }

        fun main() {
            print(Point { x = 3, y = 4 }.sum());
        }
        "#,
        "7\n",
    );
}

#[test]
fn a_struct_literal_operand_composes_with_logical_operators() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        [derive(PartialEq)]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            let p = Point { x = 1, y = 2 };
            print(Point { x = 1, y = 2 } == p && 1 < 2);
        }
        "#,
        "true\n",
    );
}

#[test]
fn a_generic_struct_literal_is_an_operand() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        [derive(PartialEq)]
        struct Holder<T> {
            value: T,
        }

        fun main() {
            let h = Holder { value = 3 };
            print(Holder<i32> { value = 3 } == h);
        }
        "#,
        "true\n",
    );
}

#[test]
fn a_parenthesized_struct_literal_serves_in_a_condition() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        [derive(PartialEq)]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            let p = Point { x = 1, y = 2 };
            if p == (Point { x = 1, y = 2 }) {
                print("equal");
            }
        }
        "#,
        "equal\n",
    );
}

#[test]
fn a_bare_struct_literal_statement_still_parses() {
    assert_compiles(
        r#"
        struct Point {
            x: i32,
        }

        fun main() {
            Point { x = 1 };
        }
        "#,
    );
}

#[test]
fn a_match_subject_does_not_take_a_struct_literal() {
    // Condition positions stay struct-free: the `{` after the subject is the
    // arms block, so a literal there is a parse error (parenthesize instead).
    assert_fails(
        r#"
        struct Point {
            x: i32,
        }

        fun main() {
            match Point { x = 1 } {
                _ => 0,
            }
        }
        "#,
    );
}

#[test]
fn a_for_iterable_does_not_take_a_struct_literal() {
    assert_fails(
        r#"
        struct Wrapper {
            items: i32,
        }

        fun main() {
            for e in Wrapper { items = 1 } { }
        }
        "#,
    );
}

// --- B.27: a bare type name is not a value --------------------------------------
// A bare name that resolves to a non-value entity — a type (struct/enum,
// primitives included), a trait, a type parameter, or a module — is rejected in
// value position (it used to compile, `let q = Point;` binding the constructor
// object). This is also what disarmed the condition-position misparse: with H.1
// keeping struct literals out of conditions, `if p == Point { .. } { .. }`
// parses `p == Point` as the condition, which now errors on `Point` instead of
// running against the type object and trapping at runtime.

#[test]
fn a_bare_struct_name_is_not_a_value() {
    assert_fails_with(
        r#"
        struct Point {
            x: i32,
        }

        fun main() {
            let q = Point;
        }
        "#,
        "`Point` is a type, not a value",
    );
}

#[test]
fn a_bare_enum_name_is_not_a_value() {
    assert_fails_with(
        r#"
        enum Color {
            Red,
            Green,
        }

        fun main() {
            let q = Color;
        }
        "#,
        "`Color` is a type, not a value",
    );
}

#[test]
fn a_bare_trait_name_is_not_a_value() {
    assert_fails_with(
        r#"
        trait Show {
        }

        fun main() {
            let q = Show;
        }
        "#,
        "`Show` is a trait, not a value",
    );
}

#[test]
fn a_bare_type_parameter_is_not_a_value() {
    // Inside an instantiated generic, `T` names a type, not a runtime value.
    assert_fails_with(
        r#"
        import std::print;

        fun identity<T>(x: T): T {
            let q = T;
            x
        }

        fun main() {
            print(identity(5));
        }
        "#,
        "`T` is a type parameter, not a value",
    );
}

#[test]
fn a_bare_primitive_name_is_not_a_value() {
    // Primitives are source `external struct`s, so they take the same path.
    assert_fails_with(
        r#"
        fun main() {
            let q = i32;
        }
        "#,
        "`i32` is a type, not a value",
    );
}

#[test]
fn a_bare_module_name_is_not_a_value() {
    assert_fails_with(
        r#"
        import std::math;

        fun main() {
            let q = math;
        }
        "#,
        "`math` is a module, not a value",
    );
}

#[test]
fn a_bare_macro_name_is_not_a_value() {
    // The family's macro arm (ledger row 120): a macro's name resolves, but
    // it is dispatch machinery, not a runtime value.
    assert_fails_with(
        r#"
        macro fun tagger(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("")
        }

        fun main() {
            let q = tagger;
        }
        "#,
        "`tagger` is a macro, not a value",
    );
}

#[test]
fn an_unparenthesized_struct_literal_condition_is_rejected_not_misparsed() {
    // The realistic shape: a user writes a struct-literal comparison in a
    // condition. H.1 parses `p == Point` (struct-free condition); B.27 then
    // rejects `Point` as a value, so it's a clear error, not a runtime trap.
    assert_fails_with(
        r#"
        import std::print;

        [derive(PartialEq)]
        struct Point {
            x: i32,
        }

        fun main() {
            let p = Point { x = 1 };
            if p == Point {
                print("y");
            }
        }
        "#,
        "`Point` is a type, not a value",
    );
}

// --- B.27 regression guards: these value forms must still compile --------------

#[test]
fn an_enum_variant_and_struct_literal_stay_values() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        enum Color {
            Red,
            Green,
        }

        [derive(PartialEq)]
        struct Point {
            x: i32,
        }

        fun main() {
            let c = Color::Red;
            print(c is Color::Red);
            let p = Point { x = 1 };
            print(p == Point { x = 1 });
        }
        "#,
        "true\ntrue\n",
    );
}

#[test]
fn a_bare_function_name_stays_a_value() {
    // B20 fn→closure coercion: a function used as a value (here coerced to a
    // closure parameter) is not rejected — only type-like names are.
    assert_compiles_and_runs(
        r#"
        import std::print;

        fun apply(f: |i32| i32, x: i32): i32 {
            f(x)
        }

        fun double(x: i32): i32 {
            x * 2
        }

        fun main() {
            print(apply(double, 21));
        }
        "#,
        "42\n",
    );
}

// --- I3: validating per-type `from_json` -----------------------------------------
// Decoding is fallible and never crashes: a missing field, a wrong-shaped value,
// or text that is not JSON is a `Result` decode error rather than `undefined`
// garbage or a thrown `JSON.parse`. Both `FromJson` methods return
// `Result<Self, str>`; the `!` operator threads a leaf failure.

#[test]
fn from_json_decodes_a_valid_scalar() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            print(i32::from_json("7") is Ok(let n) && n == 7);
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_rejects_a_wrong_typed_scalar() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            print(i32::from_json("\"x\"") is Err(let e));
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_rejects_malformed_text() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            print(i32::from_json("not json") is Err(let e) && e == "not valid JSON");
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_names_a_missing_struct_field() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::FromJson;
        import std::result::Result::{ self, Ok, Err };

        [derive(Json)]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            match Point::from_json("{\"x\":1}") {
                Ok(_) => print("?"),
                Err(let reason) => print(reason),
            }
        }
        "#,
        "missing field y\n",
    );
}

#[test]
fn from_json_rejects_a_wrong_typed_struct_field() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::FromJson;
        import std::result::Result::{ self, Ok, Err };

        [derive(Json)]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            print(Point::from_json("{\"x\":1,\"y\":\"z\"}") is Err(let e));
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_ignores_extra_struct_fields() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::FromJson;
        import std::result::Result::{ self, Ok, Err };

        [derive(Json)]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            print(Point::from_json("{\"x\":1,\"y\":2,\"z\":3}") is Ok(let p) && p.x == 1);
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_recurses_into_a_nested_struct() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::FromJson;
        import std::result::Result::{ self, Ok, Err };

        [derive(Json)]
        struct Point {
            x: i32,
        }

        [derive(Json)]
        struct Line {
            from: Point,
            to: Point,
        }

        fun main() {
            // The inner `Point` is missing its field — the failure propagates.
            print(Line::from_json("{\"from\":{\"x\":1},\"to\":{}}") is Err(let e));
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_reads_option_null_and_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            let empty: Result<Option<i32>, str> = Option::from_json("null");
            print(empty is Ok(let a) && a is None);
            let some: Result<Option<i32>, str> = Option::from_json("7");
            print(some is Ok(let b) && b is Some(let v) && v == 7);
        }
        "#,
        "true\ntrue\n",
    );
}

#[test]
fn from_json_rejects_a_non_array_for_a_list() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            let bad: Result<List<i32>, str> = List::from_json("5");
            print(bad is Err(let e) && e == "expected an array");
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_short_circuits_on_a_bad_list_element() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            let good: Result<List<i32>, str> = List::from_json("[1,2,3]");
            print(good is Ok(let xs) && xs.len() == 3);
            let bad: Result<List<i32>, str> = List::from_json("[1,\"x\",3]");
            print(bad is Err(let e));
        }
        "#,
        "true\ntrue\n",
    );
}

#[test]
fn from_json_rejects_an_unknown_enum_variant() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::FromJson;
        import std::result::Result::{ self, Ok, Err };

        [derive(Json)]
        enum Shape {
            Circle(i32),
            Empty,
        }

        fun main() {
            print(Shape::from_json("\"Triangle\"") is Err(let e));
        }
        "#,
        "true\n",
    );
}

#[test]
fn from_json_round_trips_a_derived_enum() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::FromJson;
        import std::result::Result::{ self, Ok, Err };

        [derive(Json, PartialEq)]
        enum Shape {
            Circle(i32),
            Rect(i32, i32),
            Empty,
        }

        fun main() {
            let r = Shape::Rect(2, 3);
            print(Shape::from_json(r.to_json()) is Ok(let back) && back == r);
        }
        "#,
        "true\n",
    );
}

// --- I1: value-keyed Map/Set via Hashable ---------------------------------------
// Map/Set key by value: a struct/enum/List key works (via `[derive(Hashable)]`
// or a hand-written impl), a fresh equal key hits, and `key.hash()` is dispatched
// so a custom impl is honored inside std collections.

#[test]
fn a_derived_struct_key_maps_by_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::Hashable;
        import std::option::Option::{ self, Some, None };

        [derive(Hashable)]
        struct Point { x: i32, y: i32 }

        fun main() {
            mut m: Map<Point, str> = Map::new();
            m.insert(Point { x = 1, y = 2 }, "here");
            // A FRESH, distinct-but-equal Point hits.
            match m.get(Point { x = 1, y = 2 }) {
                Some(let v) => print(v),
                None => print("miss"),
            }
            print(m.contains_key(Point { x = 9, y = 9 }));
        }
        "#,
        "here\nfalse\n",
    );
}

#[test]
fn a_set_dedups_struct_elements_by_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;

        [derive(Hashable)]
        struct Point { x: i32, y: i32 }

        fun main() {
            mut s: Set<Point> = Set::new();
            s.insert(Point { x = 1, y = 2 });
            s.insert(Point { x = 1, y = 2 });   // dup by value
            s.insert(Point { x = 3, y = 4 });
            print(s.len());                      // 2
            print(s.contains(Point { x = 1, y = 2 }));
        }
        "#,
        "2\ntrue\n",
    );
}

#[test]
fn a_derived_enum_is_a_valid_key() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;

        [derive(Hashable)]
        enum Shape { Circle(i32), Rect(i32, i32), Empty }

        fun main() {
            mut s: Set<Shape> = Set::new();
            s.insert(Shape::Circle(5));
            s.insert(Shape::Circle(5));   // dup by value
            s.insert(Shape::Empty);
            print(s.len());               // 2
            print(s.contains(Shape::Circle(5)));
        }
        "#,
        "2\ntrue\n",
    );
}

#[test]
fn a_custom_hashable_impl_is_honored_by_map() {
    // Genuine per-call dispatch: a hand-written `hash()` (by one field) is used
    // inside the std Map, so two values that hash equal collide.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::{ Hashable, Hash };

        struct User { id: i32, name: str }
        impl User with Hashable {
            fun hash(self): Hash {
                self.id.hash()
            }
        }

        fun main() {
            mut m: Map<User, str> = Map::new();
            m.insert(User { id = 1, name = "Ada" }, "a");
            m.insert(User { id = 1, name = "Bob" }, "b");   // same id -> overwrites
            print(m.len());                                  // 1
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_list_is_a_valid_key() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::Hashable;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut m: Map<List<i32>, str> = Map::new();
            m.insert([1, 2, 3], "here");
            match m.get([1, 2, 3]) {
                Some(let v) => print(v),
                None => print("miss"),
            }
        }
        "#,
        "here\n",
    );
}

#[test]
fn map_keys_and_set_iteration_return_real_values() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::set::Set;
        import std::hash::Hashable;

        [derive(Hashable, Debug)]
        struct Point { x: i32, y: i32 }

        fun main() {
            mut m: Map<Point, i32> = Map::new();
            m.insert(Point { x = 1, y = 2 }, 10);
            for key in m.keys() { print(key.debug()); }   // Point { x = 1, y = 2 }
            mut s: Set<i32> = Set::new();
            s.insert(7);
            s.insert(8);
            for x in s { print(x); }                       // 7, 8
        }
        "#,
        "Point { x = 1, y = 2 }\n7\n8\n",
    );
}

#[test]
fn a_non_hashable_field_is_rejected_by_the_derive() {
    // The all-fields check: a closure field can't be canonically hashed.
    assert_fails(
        r#"
        import std::hash::Hashable;

        [derive(Hashable)]
        struct Handler { name: str, callback: || void }

        fun main() {}
        "#,
    );
}

#[test]
fn an_aggregate_key_is_snapshot_on_insert() {
    // Value semantics: the key is copied into the map, so mutating the original
    // afterward can't desync it (§3.6).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::Hashable;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut xs: List<i32> = [1, 2];
            mut m: Map<List<i32>, str> = Map::new();
            m.insert(xs, "here");
            xs.push(3);                        // mutate the original AFTER insert
            print(m.contains_key([1, 2]));     // true  — snapshot held
            print(m.contains_key([1, 2, 3]));  // false — the mutation didn't leak
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn hashable_builds_a_reusable_container() {
    // The point of a trait-with-a-value (not a marker): a user bounds their own
    // container on `K: Hashable`, calls `key.hash()`, and keys a `Map<Hash, ..>`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::{ Hashable, Hash };
        import std::option::Option::{ self, Some, None };

        struct Counter<K: Hashable> {
            counts: Map<Hash, i32>,
        }

        impl Counter<type K: Hashable> {
            fun new(): Counter<K> {
                let counts: Map<Hash, i32> = Map::new();
                Counter { counts = counts }
            }
            fun bump(&mut self, key: K) {
                let h = key.hash();
                let current = match self.counts.get(h) {
                    Some(let n) => n,
                    None => 0,
                };
                self.counts.insert(h, current + 1);
            }
            fun count(self, key: K): i32 {
                match self.counts.get(key.hash()) {
                    Some(let n) => n,
                    None => 0,
                }
            }
        }

        [derive(Hashable)]
        struct Word { text: str }

        fun main() {
            mut c: Counter<Word> = Counter::new();
            c.bump(Word { text = "hi" });
            c.bump(Word { text = "hi" });
            c.bump(Word { text = "bye" });
            print(c.count(Word { text = "hi" }));   // 2
            print(c.count(Word { text = "bye" }));  // 1
        }
        "#,
        "2\n1\n",
    );
}

#[test]
fn b110_two_hashes_compare_with_partial_eq() {
    // hashable-keys.md §3.2/§8: `Hash` is `==`-comparable, so equal values hash
    // equal and different values do not — over a primitive, a string, and a
    // derived aggregate. The impl existed only on paper: `==` on a `Hash`
    // reported "type 'Hash' does not implement the `PartialEq` operator".
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::hash::Hashable;

        [derive(Hashable)]
        struct Point { x: i32, y: i32 }

        fun main() {
            let k = 7;
            print(k.hash() == k.hash());        // true
            print(k.hash() == 8.hash());        // false
            print("a".hash() == "a".hash());    // true
            print("a".hash() == "b".hash());    // false
            let p = Point { x = 1, y = 2 };
            print(p.hash() == Point { x = 1, y = 2 }.hash());  // true
            print(p.hash() == Point { x = 9, y = 2 }.hash());  // false
            print(p.hash() != Point { x = 9, y = 2 }.hash());  // true
        }
        "#,
        "true\nfalse\ntrue\nfalse\ntrue\nfalse\ntrue\n",
    );
}

#[test]
fn b110_hash_equality_does_not_recurse_into_its_own_impl() {
    // The reason `eq`'s body is `hashes_equal` and not `self == b`: `Hash` is
    // opaque and NOT a native-operator type, so a `==` in the body dispatches
    // back into this same impl — `function eq(self, b) { return eq(self, b); }`,
    // which stack-overflowed at runtime while compiling clean. The `.eq()` call
    // reaches the body directly, so it is the arm the operator lowering skips.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::compare::PartialEq;
        import std::hash::Hashable;

        fun main() {
            print(1.hash().eq(1.hash()));  // true
            print(1.hash().eq(2.hash()));  // false
            print(1.hash().ne(2.hash()));  // true — the inherited default
        }
        "#,
        "true\nfalse\ntrue\n",
    );
}

#[test]
fn b110_hash_satisfies_a_partial_eq_bound() {
    // What the impl buys beyond the operator: `Hash` now grounds a
    // `T: PartialEq` bound, so it nests in the conditional impls and in generic
    // user code. `Option<Hash>` is the conditional impl; `same` is the bound.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::compare::PartialEq;
        import std::hash::{ Hashable, Hash };
        import std::option::Option::{ self, Some, None };

        fun same<T: PartialEq>(a: T, b: T): bool {
            a == b
        }

        fun main() {
            print(same(1.hash(), 1.hash()));  // true
            print(same(1.hash(), 2.hash()));  // false
            let here: Option<Hash> = Some(1.hash());
            print(here == Some(1.hash()));    // true
            print(here == Some(2.hash()));    // false
            print(here == None);              // false
        }
        "#,
        "true\nfalse\ntrue\nfalse\nfalse\n",
    );
}

#[test]
fn b110_hash_is_still_not_ordered_or_arithmetic() {
    // `Hash` gained equality, not the rest of the operator surface. It is
    // deliberately absent from `is_native_operator_type` for exactly this: an
    // opaque key has no order, and `<` on one would compare canonical JSON
    // strings lexicographically.
    assert_fails_with(
        r#"
        import std::print;
        import std::hash::Hashable;

        fun main() {
            print(1.hash() < 2.hash());
        }
        "#,
        "does not implement the `PartialOrd` operator",
    );
}

// --- C5.1: a scalar view read as a value requires `*` -----------------------------
// `transparent-references.md`: `*v` is the only way to cross from view to value —
// the language never silently converts. A bare scalar view (whose runtime form is
// the `(base, key)` pair) in a value position used to leak that pair; now it's an
// error, mirroring the let-binding rule (R1).

#[test]
fn a_scalar_view_read_as_a_value_is_rejected() {
    // `print(b)` for `let b = &mut a[0]` would leak `[[99],0]`.
    assert_fails(
        r#"
        import std::print;
        fun main() {
            mut a = [99];
            let b = &mut a[0];
            print(b);
        }
        "#,
    );
}

#[test]
fn a_scalar_view_as_a_value_parameter_is_rejected() {
    assert_fails(
        r#"
        fun take_value(x: i32): i32 { x }
        fun main() {
            mut a = [99];
            let b = &mut a[0];
            let _ = take_value(b);
        }
        "#,
    );
}

#[test]
fn a_scalar_view_as_a_binary_operand_is_rejected() {
    assert_fails(
        r#"
        import std::print;
        fun main() {
            mut a = [99];
            let b = &mut a[0];
            print(b + 1);
        }
        "#,
    );
}

#[test]
fn an_explicit_deref_reads_the_scalar_view() {
    // The fix steers to `*b`, which reads the element.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut a = [99];
            let b = &mut a[0];
            print(*b);       // 99
            print(*b + 1);   // 100
        }
        "#,
        "99\n100\n",
    );
}

#[test]
fn a_scalar_view_passes_to_a_view_parameter() {
    // A view binding is still allowed as a view argument (aliasing) and for a
    // compound write-through — neither is a value read.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun bump(v: &mut i32) { v = *v + 1; }
        fun main() {
            mut a = [99];
            let b = &mut a[0];
            bump(b);      // aliasing — not a value read
            b += 5;       // compound write-through — sanctioned
            print(*b);    // 105
        }
        "#,
        "105\n",
    );
}

#[test]
fn a_mut_bool_view_writes_through() {
    // C5.3: `bool` is a numeric enum, so it used to take the aggregate view path
    // (`Object.assign`) — a no-op write. It's a scalar `(base, key)` view now.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun set_true(v: &mut bool) { v = true; }
        fun main() {
            mut flags = [false, false];
            let b = &mut flags[0];
            set_true(b);          // writes through
            print(*b);            // true
            print(flags[0]);      // true — the write reached the list
            print(flags[1]);      // false — untouched
        }
        "#,
        "true\ntrue\nfalse\n",
    );
}

#[test]
fn a_mut_bool_view_toggles_through_a_negated_deref() {
    // C5.3 + the operator-lexer fix: the natural thing to do with a `&mut bool`
    // view is toggle it, `v = !*v`. That failed to *parse* before — the lexer
    // fused `!*` into one bogus token — so the scalar-bool view shipped without
    // an ergonomic toggle. Now it reads through (`*v`), negates, and writes back.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun toggle(v: &mut bool) { v = !*v; }
        fun main() {
            mut flags = [true, false];
            toggle(&mut flags[0]);   // transient views — none outlive its call
            toggle(&mut flags[1]);
            print(flags[0]);   // false
            print(flags[1]);   // true
        }
        "#,
        "false\ntrue\n",
    );
}

#[test]
fn a_mut_bool_view_of_a_scalar_local_writes_through() {
    // C5.3 gap (found verifying the v0.6.0 release): a view of a scalar *local*
    // must box the local to `[value]` so the `(base, key)` pair has a real cell.
    // `bool` is a numeric enum, so `compute_boxed_locals` (keyed on
    // `is_scalar_primitive`, structs only) skipped it — `&mut b` lowered to
    // `[b, 0]` over the raw value and the write-through no-oped. The earlier bool
    // pins used list elements (base already an object), so they missed it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun toggle(v: &mut bool) { v = !*v; }
        fun main() {
            mut b = true;
            toggle(&mut b);      // through a call
            print(b);            // false
            let w = &mut b;      // direct local view
            w = true;
            print(b);            // true
        }
        "#,
        "false\ntrue\n",
    );
}

#[test]
fn a_mut_view_through_a_generic_param_writes_through_for_every_scalar() {
    // A generic `&mut T` param's pointee is abstract in the analyzer, so the
    // scalar-vs-aggregate view lowering is re-decided in the transformer at each
    // monomorphization (`resolves_to_scalar_view_pointee`). That check carried its
    // own copy of the scalar names and never grew `bool` (a numeric enum), so a
    // generic `&mut T` resolving to `bool` took the aggregate `Object.assign` path
    // — a silent no-op — while `i32`/`str` wrote through. Pins both kinds (a scalar
    // struct and the bool enum) so the analyzer and transformer can't drift again.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun set<T>(v: &mut T, x: T) { v = x; }
        fun main() {
            mut n = 1;
            set(&mut n, 42);
            print(n);            // 42 — scalar struct

            mut s = "a";
            set(&mut s, "b");
            print(s);            // b — str

            mut flag = true;
            set(&mut flag, false);
            print(flag);         // false — bool enum (the regression)
        }
        "#,
        "42\nb\nfalse\n",
    );
}

// --- Fixed-length arrays `[T; n]` (proposal/fixed-arrays.md) ---------------------

#[test]
fn fixed_array_repeat_literal_and_indexing() {
    // `[value; n]` builds a fixed array; scalar values fill, and indexing reads.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let zeros = [0; 4];        // [i32; 4]
            print(zeros[0]);           // 0
            mut buf: [i32; 3] = [1, 2, 3];  // context-directed list literal
            buf[1] = 20;               // index write
            print(buf[1]);             // 20
            print(buf[0] + buf[2]);    // 4
        }
        "#,
        "0\n20\n4\n",
    );
}

#[test]
fn fixed_array_repeat_of_an_aggregate_copies_each_slot() {
    // `[value; n]` for an aggregate clones the value into each slot, so the slots
    // are independent (value semantics) — mutating one leaves the others.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Cell { n: i32 }
        fun main() {
            mut cells = [Cell { n = 7 }; 3];
            cells[0].n = 99;
            print(cells[0].n);   // 99
            print(cells[1].n);   // 7 — independent
            print(cells[2].n);   // 7
        }
        "#,
        "99\n7\n7\n",
    );
}

#[test]
fn fixed_array_value_copy_is_independent() {
    // A fixed array is a value: `let b = a` deep-copies, so a later write to `a`
    // leaves `b` untouched.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut a: [i32; 3] = [1, 2, 3];
            let b = a;
            a[0] = 99;
            print(b[0]);   // 1
            print(a[0]);   // 99
        }
        "#,
        "1\n99\n",
    );
}

#[test]
fn fixed_array_element_view_writes_through() {
    // `&mut arr[i]` is an element view — writing through it reaches the array.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun bump(v: &mut i32) { v += 100; }
        fun main() {
            mut buf: [i32; 3] = [1, 2, 3];
            let v = &mut buf[1];
            bump(v);
            print(buf[1]);   // 102
        }
        "#,
        "102\n",
    );
}

#[test]
fn fixed_array_iteration_params_returns_and_nesting() {
    // `for x in arr` iterates the elements; arrays pass as parameters and returns;
    // and `[[T; m]; n]` nests.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun total(a: [i32; 3]): i32 {
            mut sum = 0;
            for x in a { sum = sum + x; }
            sum
        }
        fun make(): [i32; 2] { [5; 2] }
        fun main() {
            print(total([1, 2, 3]));   // 6
            let m = make();
            print(m[0] + m[1]);        // 10
            let grid: [[i32; 2]; 2] = [[1, 2], [3, 4]];
            print(grid[1][0]);         // 3
        }
        "#,
        "6\n10\n3\n",
    );
}

#[test]
fn fixed_array_literal_index_out_of_range_is_a_compile_error() {
    // The length is in the type, so a literal index proven out of range is caught
    // at compile time (a dynamic index keeps its runtime bounds check).
    assert_fails_spanning(
        r#"
        fun main() {
            let a: [i32; 4] = [1, 2, 3, 4];
            let x = a[9];
        }
        "#,
        "a[9]",
        "out of range for an array of length 4",
    );
}

#[test]
fn fixed_arrays_of_different_lengths_are_distinct_types() {
    // The length is part of the type — `[i32; 3]` is not `[i32; 4]`.
    assert_fails(
        r#"
        fun main() {
            let a: [i32; 3] = [1, 2, 3];
            let b: [i32; 4] = a;
        }
        "#,
    );
}

#[test]
fn context_directed_array_literal_count_must_match() {
    // A list literal directed to `[T; n]` must have exactly `n` elements.
    assert_fails(
        r#"
        fun main() {
            let a: [i32; 3] = [1, 2];
        }
        "#,
    );
}

#[test]
fn context_directed_array_literal_elements_must_be_t() {
    // The direction arm returns the expected array type, so it must CHECK each
    // element against `T` — without the check a stray `str` in an `[i32; n]`
    // sailed straight through the annotation.
    assert_fails_spanning(
        r#"
        fun main() {
            let a: [i32; 2] = [1, "x"];
        }
        "#,
        r#""x""#,
        "Expected i32 (this literal's element type), but got str instead.",
    );
}

#[test]
fn a_heterogeneous_list_literal_is_rejected() {
    // The element reconcile chain used to swallow a mismatch silently, typing
    // the literal by its FIRST element — `[1, "x"]` became a `List<i32>` with a
    // `str` inside, and reads through it were unsound. Now each element that
    // fails to unify reports, annotated or not.
    assert_fails_spanning(
        r#"
        fun main() {
            let a = [1, "x"];
        }
        "#,
        r#""x""#,
        "Expected i32 (this literal's element type), but got str instead.",
    );
}

#[test]
fn an_annotated_heterogeneous_list_literal_is_rejected() {
    assert_fails(
        r#"
        fun main() {
            let a: List<i32> = [1, "x"];
        }
        "#,
    );
}

#[test]
fn a_mixed_literal_under_a_list_of_any_parameter_is_legitimate() {
    // The std::db shape: `run(parameters: List<any>)` takes a deliberately mixed
    // parameter list. An element the EXPECTED element type absorbs is not a
    // mismatch — the check consults the `List<T>` expectation before reporting.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun describe(values: List<any>): i32 {
            values.len()
        }
        fun main() {
            print(describe(["write the pilot", 0]));   // 2 — str + i32, absorbed by any
        }
        "#,
        "2\n",
    );
}

#[test]
fn an_array_annotation_catches_elements_that_unify_with_each_other() {
    // The array arm's own element check still matters when the elements DO
    // unify with each other but not with `T`: `[1, 2]` unifies to i32, which the
    // list-level check can't fault — only the `[str; 2]` direction can.
    assert_fails_spanning(
        r#"
        fun main() {
            let a: [str; 2] = [1, 2];
        }
        "#,
        "1",
        "Expected str (this literal's element type), but got i32 instead.",
    );
}

// --- Fixed-array destructuring `let [a, b, c] = arr` (fixed-arrays.md §7) --------

#[test]
fn fixed_array_destructuring_binds_elements() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let rgb: [i32; 3] = [255, 128, 0];
            let [r, g, b] = rgb;
            print(r + g + b);   // 383
        }
        "#,
        "383\n",
    );
}

#[test]
fn fixed_array_destructuring_nests_and_copies() {
    // Nested array patterns, a `mut` pattern (every binding mutable), and
    // value semantics: the destructured copies are independent of the source.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut source: [[i32; 2]; 2] = [[1, 2], [3, 4]];
            let [first, second] = source;
            let [c, d] = first;
            print(c + d);          // 3
            mut [x, y] = second;
            x = x + 100;
            print(x);              // 103
            print(y);              // 4
            print(source[1][0]);   // 3 — the source is untouched
        }
        "#,
        "3\n103\n4\n3\n",
    );
}

#[test]
fn fixed_array_destructuring_of_aggregate_elements_is_a_copy() {
    // An aggregate element clones on the way out (rule 1): mutating the
    // binding leaves the source array's element unchanged.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Cell { n: i32 }
        fun main() {
            let cells: [Cell; 2] = [Cell { n = 1 }, Cell { n = 2 }];
            mut [a, b] = cells;
            a.n = 99;
            print(a.n);           // 99
            print(cells[0].n);    // 1 — independent
            print(b.n);           // 2
        }
        "#,
        "99\n1\n2\n",
    );
}

#[test]
fn fixed_array_destructuring_in_parameter_position() {
    // Binder patterns are shared between `let` and parameters, and a tuple
    // pattern nests inside an array pattern (flat tuple reads under an
    // indexed element read).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun sum([a, b]: [i32; 2]): i32 { a + b }
        fun main() {
            print(sum([40, 2]));   // 42
            let pairs: [(i32, str); 2] = [(1, "a"), (2, "b")];
            let [(n1, s1), (n2, s2)] = pairs;
            print(n1 + n2);        // 3
            print(s1 + s2);        // ab
        }
        "#,
        "42\n3\nab\n",
    );
}

#[test]
fn fixed_array_destructuring_count_must_match() {
    assert_fails_with(
        r#"
        fun main() {
            let a: [i32; 3] = [1, 2, 3];
            let [x, y] = a;
        }
        "#,
        "this pattern binds 2 elements, but the array's length is 3",
    );
}

#[test]
fn fixed_array_destructuring_rejects_a_list() {
    // A List's length isn't in its type, so `[a, b]` can't be irrefutable
    // over it — the pattern is for `[T; n]` only.
    assert_fails_with(
        r#"
        fun main() {
            let xs = [1, 2];
            let [a, b] = xs;
        }
        "#,
        "cannot destructure List<i32> as a fixed array",
    );
}

// --- `[T; n].len()` — the fold (fixed-arrays.md §10) -----------------------------

#[test]
fn fixed_array_len_folds_to_the_constant_and_types_as_i32() {
    // `arr.len()` is the compile-time length, typed `i32` (like `List.len()`),
    // so it participates in arithmetic and satisfies an `i32` annotation.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let a = [0; 4];
            let n: i32 = a.len();
            print(n);             // 4
            print(a.len() + 1);   // 5
        }
        "#,
        "4\n5\n",
    );
}

#[test]
fn fixed_array_len_on_nested_arrays_and_through_a_view() {
    // The outer length, the inner length through a subscript (which keeps its
    // bounds check — the side-effectful emission path), and a `for … in &grid`
    // view binder (views type transparently).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let grid: [[i32; 2]; 3] = [[1, 2], [3, 4], [5, 6]];
            print(grid.len());      // 3
            print(grid[0].len());   // 2
            for row in &grid {
                print(row.len());   // 2, three times
            }
        }
        "#,
        "3\n2\n2\n2\n2\n",
    );
}

#[test]
fn fixed_array_len_evaluates_a_side_effectful_subject_once() {
    // A call subject must still run — exactly once — even though the result's
    // length is known statically (the emission reads `subject.length` in place
    // rather than folding the subject away).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make(log: &mut List<i32>): [i32; 2] {
            log.push(7);
            [5; 2]
        }
        fun main() {
            mut log: List<i32> = [];
            print(make(&mut log).len());   // 2
            print(log.len());              // 1 — the subject ran once
        }
        "#,
        "2\n1\n",
    );
}

#[test]
fn fixed_array_len_takes_no_arguments() {
    assert_fails_with(
        r#"
        fun main() {
            let a = [0; 4];
            let n = a.len(1);
        }
        "#,
        "`len` takes no arguments",
    );
}

#[test]
fn an_array_has_no_method_besides_len() {
    // No `push` — the contract is "exactly `n`, always"; the standard
    // no-method error names the array type.
    assert_fails_with(
        r#"
        fun main() {
            mut a = [0; 4];
            a.push(1);
        }
        "#,
        "has no method 'push'",
    );
}

#[test]
fn an_unused_repeat_of_a_side_effectful_value_still_runs() {
    // `[value; n]` evaluates its value once, so an unused binding whose
    // initializer is a repeat of a CALL cannot be elided — the call's side
    // effect must land (`expr_has_side_effects` recurses into the repeat).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun bump(log: &mut List<i32>): i32 {
            log.push(1);
            0
        }
        fun main() {
            mut log: List<i32> = [];
            let unused = [bump(&mut log); 3];
            print(log.len());   // 1 — evaluated once, not dropped, not per-slot
        }
        "#,
        "1\n",
    );
}

// --- Parser diagnostics (diagnostics-standard.md §4: targeted labels/hints
// --- from the handwritten frontend — `parsing::parse` + `parsing::render`)

/// The `!=` soup: `a!==b` lexes as `!=` then `=`. The parse error carries the
/// targeted hint naming the real fix.
#[test]
fn the_not_equals_soup_hints_the_postfix_bang_spacing() {
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };
        fun main() {
            let a = Some(1);
            let bad = a!==None;
        }
        "#,
        "the space is required: `a! == b`",
    );
}

// --- E101: three diagnostics that named no cause ---------------------------
//
// Each reported something true about the token stream and nothing about the
// mistake — `pub fun` as a missing `;` three columns in, `let mut x` as a `let`
// that is not a statement, a literal brace in an i-string as a failure inside
// an expression the author never wrote. Each now names its cause and the
// sanctioned spelling (diagnostics-standard.md B6), recognized structurally.

#[test]
fn a_pub_marker_names_itself_instead_of_a_missing_semicolon() {
    assert_fails_spanning(
        r#"
pub fun helper(): i32 { 1 }

fun main() {
    let _ = helper();
}
        "#,
        "pub",
        "`pub` is not a vilan keyword",
    );
}

#[test]
fn a_pub_marker_no_longer_asks_for_a_semicolon() {
    // The half that matters: the misleading message is GONE, not merely joined
    // by a better one.
    assert_fails_without(
        r#"
pub struct Point { x: i32 }

fun main() {}
        "#,
        "expected `;` to end this statement",
    );
}

#[test]
fn the_pub_steer_names_the_export_form_it_is_not() {
    assert_fails_with(
        r#"
public fun helper(): i32 { 1 }

fun main() {}
        "#,
        "`export` exists, but it RE-exports",
    );
}

#[test]
fn a_bare_pub_identifier_is_still_an_ordinary_name() {
    // The negative: `pub` is an identifier, and the steer fires only where one
    // stands immediately before a fresh statement or item.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pub = 1;
            print(pub);
        }
        "#,
        "1
",
    );
}

#[test]
fn let_mut_names_the_two_binding_forms() {
    assert_fails_spanning(
        r#"
        import std::print;
        fun main() {
            let mut x = 1;
            x = 2;
            print(x);
        }
        "#,
        "let mut",
        "a mutable binding is spelled `mut x = …`",
    );
}

#[test]
fn let_mut_no_longer_reports_the_let_as_a_non_statement() {
    assert_fails_without(
        r#"
        fun main() {
            let mut x = 1;
        }
        "#,
        "found 'let' expected",
    );
}

#[test]
fn a_literal_brace_in_an_istring_names_the_escape() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            print(i"body { color: red }");
        }
        "#,
        r"opens an interpolation hole",
    );
}

#[test]
fn an_empty_brace_pair_in_an_istring_names_the_escape_too() {
    // The shape the evidence run hit: `{}` reads as an EMPTY hole, so the
    // located failure is "found ')' expected an expression" — about a hole
    // nobody wrote.
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            print(i"a{}b");
        }
        "#,
        r"write `\{` (and `\}`) for a literal brace",
    );
}

#[test]
fn an_escaped_brace_in_an_istring_still_prints_the_brace() {
    // The negative for the note: the sanctioned spelling it names works, and a
    // real hole beside it still interpolates.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let n = 42;
            print(i"n = {n} and \{literal\}");
        }
        "#,
        "n = 42 and {literal}
",
    );
}

#[test]
fn a_broken_expression_outside_a_hole_gets_no_brace_note() {
    // The other negative: the note is scoped to a failure INSIDE a hole, so an
    // ordinary parenthesized expression that breaks is unchanged.
    assert_fails_without(
        r#"
        fun main() {
            let bad = (1 + );
        }
        "#,
        "opens an interpolation hole",
    );
}

/// An unclosed generic argument list steers to `,` or `>` without the
/// optional-continuation noise (`context clause`, `generic arguments`) chumsky
/// would offer, and names the type position it failed in.
#[test]
fn an_unclosed_generic_steers_to_comma_or_close() {
    let source = r#"
        fun main() {
            let pairs: Map<str, List<i32> = Map::new();
        }
        "#;
    assert_fails_with(source, "expected ',' or '>' in type");
    match compile(source) {
        Ok(_) => panic!("expected a parse error"),
        Err(errors) => {
            assert!(
                errors.iter().all(|error| !error.contains("context clause")
                    && !error.contains("generic arguments")),
                "optional-continuation noise leaked: {errors:#?}"
            )
        }
    }
}

/// A missing comma between parameters steers to `,` or `)` — the
/// grammatically-admissible-but-never-the-fix continuations are dropped.
#[test]
fn a_missing_parameter_comma_steers_to_comma_or_close() {
    let source = r#"
        fun f(x: i32 y: i32) {}
        fun main() { f(1, 2); }
        "#;
    assert_fails_with(source, "expected ',' or ')'");
    match compile(source) {
        Ok(_) => panic!("expected a parse error"),
        Err(errors) => assert!(
            errors
                .iter()
                .all(|error| !error.contains("generic arguments")),
            "optional-continuation noise leaked: {errors:#?}"
        ),
    }
}

// --- Tuple bounds on generics (variadic-generics.md "Arity & element
// --- bounds"; backlog B3) — parsed since the variadic arc, ENFORCED now.

#[test]
fn an_arity_lower_bound_rejects_a_short_tuple() {
    assert_fails_with(
        r#"
        fun needs_three<T: (3..)>(items: T) {}
        fun main() {
            needs_three((1, 2));
        }
        "#,
        "has 2 elements: the bound '(3..)' requires at least 3",
    );
}

#[test]
fn an_arity_upper_bound_rejects_a_long_tuple() {
    assert_fails_with(
        r#"
        fun at_most_two<T: (..2)>(items: T) {}
        fun main() {
            at_most_two((1, 2, 3));
        }
        "#,
        "has 3 elements: the bound '(..2)' allows at most 2",
    );
}

#[test]
fn a_non_tuple_argument_names_the_tuple_bound() {
    assert_fails_with(
        r#"
        fun needs_tuple<T: (2..)>(items: T) {}
        fun main() {
            needs_tuple(5);
        }
        "#,
        "'i32' is not a tuple: this argument's parameter is bound '(2..)'",
    );
}

#[test]
fn a_satisfying_tuple_passes_its_arity_bound() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun arity_ok<T: (2..)>(items: T): i32 {
            42
        }
        fun main() {
            print(arity_ok((7, 8, 9)));
        }
        "#,
        "42\n",
    );
}

#[test]
fn an_element_bound_rejects_a_non_conforming_element() {
    assert_fails_with(
        r#"
        trait Label {
            fun label(self): str;
        }
        struct Tag {}
        impl Tag with Label {
            fun label(self): str {
                "tag"
            }
        }
        fun all_labels<T: (..: Label)>(items: T) {}
        fun main() {
            all_labels((Tag {}, 5));
        }
        "#,
        "element 1 of '(Tag, i32)' is 'i32', which does not implement trait 'Label'",
    );
}

#[test]
fn conforming_elements_pass_their_element_bound() {
    assert_compiles(
        r#"
        trait Label {
            fun label(self): str;
        }
        struct Tag {}
        impl Tag with Label {
            fun label(self): str {
                "tag"
            }
        }
        fun all_labels<T: (2..: Label)>(items: T) {}
        fun main() {
            all_labels((Tag {}, Tag {}));
        }
        "#,
    );
}

// Forwarding a generic into a tuple-bounded position: only the forwarded
// parameter's OWN tuple bound can guarantee the callee's.
#[test]
fn a_forwarded_generic_without_a_tuple_bound_is_rejected() {
    assert_fails_with(
        r#"
        fun needs_two<T: (2..)>(items: T) {}
        fun outer<U>(x: U) {
            needs_two(x);
        }
        fun main() {
            outer((1, 2));
        }
        "#,
        "generic parameter 'U' is missing the tuple bound '(2..)'",
    );
}

#[test]
fn a_forwarded_generic_with_a_weaker_range_is_rejected() {
    assert_fails_with(
        r#"
        fun needs_two<T: (2..)>(items: T) {}
        fun outer<U: (1..)>(x: U) {
            needs_two(x);
        }
        fun main() {
            outer((1, 2));
        }
        "#,
        "is bound '(1..)', which does not guarantee the tuple bound '(2..)'",
    );
}

#[test]
fn a_forwarded_generic_with_a_contained_bound_is_accepted() {
    assert_compiles(
        r#"
        fun needs_two<T: (2..)>(items: T) {}
        fun outer<U: (3..)>(x: U) {
            needs_two(x);
        }
        fun main() {
            outer((1, 2, 3));
        }
        "#,
    );
}

// Construction sites check the declaration's tuple bound too, independent of
// any call.
#[test]
fn a_struct_construction_checks_its_tuple_bound() {
    assert_fails_with(
        r#"
        struct Pack<T: (..2)> {
            items: T,
        }
        fun main() {
            let packed = Pack { items = (1, 2, 3) };
        }
        "#,
        "has 3 elements: the bound '(..2)' allows at most 2",
    );
}

// --- Spread parameters (`fun log(...items: T)`; backlog B3,
// --- proposal/variadic-generics.md §S). `...` is a CALL CONVENTION over an
// --- ordinary tuple parameter: `fun f(...items: T) {b}` is `fun f(items: T)
// --- {b}` with `f(a, b)` meaning `f((a, b))`. Every pin below is the desugar
// --- plus something that already held of tuples — which is the point.

/// The collected arguments land in the pack's slots IN ORDER — read back
/// positionally at a concrete pack type. (A pack that is still an abstract
/// `T` cannot be indexed; see the note at the end of this block.)
#[test]
fn a_spread_call_collects_its_arguments_into_the_pack() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun middle(...items: (i32, i32, i32)): i32 {
            items.1
        }
        fun main() {
            print(middle(4, 5, 6));
        }
        "#,
        "5\n",
    );
}

/// One monomorphization per arity, including the two arities SOURCE SYNTAX
/// cannot write as a value: the empty pack `()` and the one-tuple `(x)`. A
/// spread parameter is the first thing in the language that can produce them.
#[test]
fn a_spread_parameter_accepts_every_arity_its_bound_admits() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun width<T: (..)>(...items: T): i32 {
            1
        }
        fun main() {
            print(width());
            print(width(1));
            print(width(1, 2));
            print(width("a", true, 3, 4));
        }
        "#,
        "1\n1\n1\n1\n",
    );
}

/// The empty pack emits the empty tuple, and the one-element pack a
/// one-element one — the flat storage the tuple form already uses.
#[test]
fn the_empty_and_one_element_packs_emit_their_tuples() {
    let source = r#"
        import std::print;
        fun width<T: (..)>(...items: T): i32 {
            1
        }
        fun main() {
            print(width());
            print(width(9));
        }
        "#;
    assert_emits_containing(source, "([  ])");
    assert_emits_containing(source, "([ 9 ])");
}

/// Fixed parameters are matched positionally first; the spread takes the rest.
#[test]
fn a_spread_follows_the_fixed_parameters() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun after<T: (..)>(head: i32, ...rest: T): i32 {
            head
        }
        fun main() {
            print(after(1));
            print(after(2, 3, 4));
        }
        "#,
        "1\n2\n",
    );
}

/// Fewer arguments than there are FIXED parameters says "at least" — a
/// variadic signature has no exact count to name.
#[test]
fn too_few_arguments_for_the_fixed_parameters_says_at_least() {
    assert_fails_with(
        r#"
        fun after<T: (..)>(head: i32, ...rest: T): i32 {
            head
        }
        fun main() {
            after();
        }
        "#,
        "Expected at least 1 argument, but got 0 instead.",
    );
}

/// Arity is INHERITED from the tuple bound, not reinvented: the shipped check
/// fires on the collected pack, note and all.
#[test]
fn a_spread_call_below_the_arity_bound_is_rejected() {
    assert_fails_with(
        r#"
        fun pair_up<T: (2..)>(...items: T): i32 {
            1
        }
        fun main() {
            pair_up(1);
        }
        "#,
        "'(i32)' has 1 element: the bound '(2..)' requires at least 2",
    );
}

#[test]
fn a_spread_call_above_the_arity_bound_is_rejected() {
    assert_fails_with(
        r#"
        fun at_most_two<T: (..2)>(...items: T): i32 {
            1
        }
        fun main() {
            at_most_two(1, 2, 3);
        }
        "#,
        "has 3 elements: the bound '(..2)' allows at most 2",
    );
}

/// Inference THROUGH the pack: each collected argument's type unifies into
/// `T`'s elements, and the element bound is checked per element.
#[test]
fn a_spread_pack_unifies_its_elements_against_the_element_bound() {
    assert_fails_with(
        r#"
        trait Label {
            fun label(self): str;
        }
        struct Tag {}
        impl Tag with Label {
            fun label(self): str {
                "tag"
            }
        }
        struct Plain {}
        fun labelled<T: (..: Label)>(...items: T): i32 {
            1
        }
        fun main() {
            labelled(Tag {}, Plain {});
        }
        "#,
        "does not implement trait 'Label'",
    );
}

#[test]
fn conforming_elements_pass_a_spread_element_bound() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait Label {
            fun label(self): str;
        }
        struct Tag {}
        impl Tag with Label {
            fun label(self): str {
                "tag"
            }
        }
        fun labelled<T: (..: Label)>(...items: T): i32 {
            7
        }
        fun main() {
            print(labelled(Tag {}, Tag {}));
        }
        "#,
        "7\n",
    );
}

/// The payoff: a MAPPED pack. `gather(a, b)` inverts `(U in T: Signal<U>)`
/// against the collected `(Signal<i32>, Signal<str>)` to recover
/// `T = (i32, str)` — the shipped inversion, reached through the spread.
#[test]
fn a_mapped_spread_pack_inverts_to_its_source_tuple() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::reactive::Signal;
        fun gather<T: (2..)>(...sources: (U in T: Signal<U>)): Signal<T> {
            let snapshot = || (source in sources => source.get());
            let derived = Signal::new(snapshot());
            let subscriptions = (source in sources => source.sub(|_| {
                derived.set(snapshot());
            }));
            derived
        }
        fun main() {
            let count = Signal::new(10);
            let name = Signal::new("hi");
            let both = gather(count, name);
            print(both.get().0);
            count.set(11);
            print(both.get().0);
            print(both.get().1);
        }
        "#,
        "10\n11\nhi\n",
    );
}

/// Emission has no spread path of its own: a call emits the flat tuple
/// construction the tuple form already emitted, and a TUPLE-TYPED argument
/// splices its slots in rather than nesting.
#[test]
fn a_spread_call_emits_the_flat_tuple_construction() {
    assert_emits_containing(
        r#"
        import std::print;
        fun width<T: (..)>(...items: T): i32 {
            1
        }
        fun main() {
            let inner = (4, 5);
            print(width(inner, 6));
        }
        "#,
        "([ ...inner, 6 ])",
    );
}

/// `mut` is binder mutability, not a convention, and the collected tuple is
/// the callee's own value — so the desugar carries it through (§S.5).
#[test]
fn a_spread_parameter_may_be_declared_mut() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun rebind<T: (..)>(mut items: T, n: i32): i32 {
            n
        }
        fun swap_pack(mut ...items: (i32, i32)): i32 {
            items = (8, 9);
            items.0
        }
        fun main() {
            print(rebind((1, 2), 3));
            print(swap_pack(1, 2));
        }
        "#,
        "3\n8\n",
    );
}

/// §S.8: the convention lives on the DECLARATION, so a spread function used as
/// a VALUE has its tuple type — and the callback calls it with a tuple.
#[test]
fn a_spread_function_passed_as_a_value_has_the_tuple_form() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun count(...items: (i32, i32)): i32 {
            items.0
        }
        fun apply(f: sync |(i32, i32)| i32): i32 {
            f((4, 5))
        }
        fun main() {
            print(count(1, 2));
            print(apply(count));
        }
        "#,
        "1\n4\n",
    );
}

/// §S.8: choosing `...` chooses the convention. A tuple written at a spread
/// call site is collected like every argument — into a ONE-element pack.
#[test]
fn a_tuple_written_at_a_spread_call_becomes_a_one_element_pack() {
    assert_fails_with(
        r#"
        fun pair_up<T: (2..)>(...items: T): i32 {
            1
        }
        fun main() {
            let pair = (1, 2);
            pair_up(pair);
        }
        "#,
        "'((i32, i32))' has 1 element: the bound '(2..)' requires at least 2",
    );
}

// --- The refusals (§S.3, §S.5, §S.6, §S.7). Each carries its own steer;
// --- pinning the message keeps the steer from rotting into a bare rejection.

#[test]
fn a_spread_must_be_the_last_parameter() {
    assert_fails_with(
        r#"
        fun misplaced<T: (..)>(...items: T, extra: i32): i32 {
            extra
        }
        fun main() {
            misplaced(1, 2);
        }
        "#,
        "must be the last parameter (and there can be only one)",
    );
}

#[test]
fn two_spread_parameters_are_rejected() {
    assert_fails_with(
        r#"
        fun twice<A: (..), B: (..)>(...first: A, ...second: B): i32 {
            1
        }
        fun main() {
            twice(1, 2);
        }
        "#,
        "must be the last parameter (and there can be only one)",
    );
}

#[test]
fn a_spread_parameter_refuses_a_convention() {
    for prefix in ["own ", "&", "&mut "] {
        assert_fails_with(
            &format!(
                r#"
                fun taken<T: (..)>({prefix}...items: T): i32 {{
                    1
                }}
                fun main() {{
                    taken(1);
                }}
                "#
            ),
            "so there is nothing for `own` or a view (`&`, `&mut`) to transfer or alias",
        );
    }
}

/// The convention may also arrive from the TYPE (`...items: &T`), which is the
/// arm that would otherwise slip past a prefix-only check.
#[test]
fn a_spread_parameter_refuses_a_view_type() {
    assert_fails_with(
        r#"
        fun viewed(...items: &(i32, i32)): i32 {
            1
        }
        fun main() {
            viewed(1, 2);
        }
        "#,
        "so there is nothing for `own` or a view (`&`, `&mut`) to transfer or alias",
    );
}

#[test]
fn a_spread_parameter_must_declare_its_pack_type() {
    assert_fails_with(
        r#"
        fun untyped(...items): i32 {
            1
        }
        fun main() {
            untyped(1);
        }
        "#,
        "a spread parameter must declare its pack type",
    );
}

#[test]
fn a_spread_parameter_refuses_a_destructuring_binder() {
    assert_fails_with(
        r#"
        fun split(...(head, tail): (i32, i32)): i32 {
            head
        }
        fun main() {
            split(1, 2);
        }
        "#,
        "binds the whole pack to a plain name; destructure it in the body",
    );
}

/// §S.6 — decided, not silent. A closure TYPE has no variadic form, so a
/// variadic closure could not be annotated, stored, or passed anywhere.
#[test]
fn a_closure_refuses_a_spread_parameter() {
    assert_fails_with(
        r#"
        fun main() {
            let log = |...items: (i32, i32)| items.0;
        }
        "#,
        "a closure cannot take a spread parameter",
    );
}

/// §S.7 — unlike `mut`, `...` IS part of the signature, so conformance may not
/// see a trait declaration and its impl disagree about it. Refused at BOTH.
#[test]
fn a_trait_method_declaration_refuses_a_spread_parameter() {
    assert_fails_with(
        r#"
        trait Log {
            fun emit<T: (..)>(self, ...items: T): i32;
        }
        fun main() {}
        "#,
        "a spread parameter is only available on a free `fun`",
    );
}

#[test]
fn a_trait_impl_method_refuses_a_spread_parameter() {
    assert_fails_with(
        r#"
        trait Log {
            fun emit(self, items: (i32, i32)): i32;
        }
        struct Console {}
        impl Console with Log {
            fun emit(self, ...items: (i32, i32)): i32 {
                items.0
            }
        }
        fun main() {}
        "#,
        "a spread parameter is only available on a free `fun`",
    );
}

#[test]
fn an_inherent_method_refuses_a_spread_parameter() {
    assert_fails_with(
        r#"
        struct Console {}
        impl Console {
            fun emit<T: (..)>(self, ...items: T): i32 {
                1
            }
        }
        fun main() {}
        "#,
        "a spread parameter is only available on a free `fun`",
    );
}

/// Memberhood belongs to the impl/trait ITEM LIST, not to everything
/// lexically inside it: a free `fun` declared in a member's body is still a
/// free `fun`, and takes a spread like any other.
#[test]
fn a_function_nested_in_a_member_body_still_takes_a_spread() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point {
            x: i32,
        }
        impl Point {
            fun go(self): i32 {
                fun helper(...items: (i32, i32)): i32 {
                    items.1
                }
                helper(1, 2)
            }
        }
        fun main() {
            print(Point { x = 0 }.go());
        }
        "#,
        "2\n",
    );
}

#[test]
fn an_external_fun_refuses_a_spread_parameter() {
    assert_fails_with(
        r#"
        external fun host_log(...items: (i32, i32)): i32;
        fun main() {}
        "#,
        "an `external fun` binds a host function, whose calling convention is the host's",
    );
}

/// §S.4 — the OTHER direction, SHIPPED one slice later by the tuple-value
/// spread (§T.6). It needed no new call-site machinery: the collection already
/// builds an `Expr::Tuple` from the arguments, so a spread argument lands
/// inside it and the call proceeds as `pair_up((..pair))`.
#[test]
fn spreading_a_tuple_at_the_call_site_forwards_the_pack() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun pair_up<T: (2..)>(...items: T): i32 {
            1
        }
        fun main() {
            let pair = (1, 2);
            print(pair_up(..pair));
        }
        "#,
        "1\n",
    );
}

/// A spread parameter is a plain tuple parameter from the inside, so
/// forwarding the pack to a tuple-parameter callee just works — which is also
/// how a pack is forwarded at all, the caller direction being deferred (§S.4).
#[test]
fn a_pack_forwards_to_a_tuple_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun inner(items: (i32, i32)): i32 {
            items.0
        }
        fun outer(...items: (i32, i32)): i32 {
            inner(items)
        }
        fun main() {
            print(outer(3, 4));
        }
        "#,
        "3\n",
    );
}

/// An unannotated closure argument makes the call-subject constraint DEFER and
/// retry, so this is the collection running more than once for one call: each
/// closure takes its type from the pack's declared slot, and the pack that
/// finally wires is the one the arguments were typed against.
#[test]
fn unannotated_closures_in_a_pack_type_from_their_slots() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun apply_both(...items: (|i32| i32, |i32| i32)): i32 {
            items.0(1) + items.1(2)
        }
        fun main() {
            print(apply_both(|n| n + 10, |n| n * 3));
        }
        "#,
        "17\n",
    );
}

/// A spread call inside a GENERIC function: the pack is collected once per
/// instantiation, so `T` is a different tuple in each.
#[test]
fn a_spread_call_inside_a_generic_collects_per_instantiation() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun width<T: (..)>(...items: T): i32 {
            1
        }
        fun through<A>(value: A): i32 {
            width(value, 5)
        }
        fun main() {
            print(through("s"));
            print(through(7));
        }
        "#,
        "1\n1\n",
    );
}

/// A PRE-EXISTING limit the pack inherits, pinned here so it is on the record
/// rather than mistaken for a spread bug: positional access on a pack that is
/// still an abstract tuple-bounded `T` is refused, because the body
/// type-checks once before any arity is known. It fails identically on the
/// tuple form (`fun first<T: (2..)>(items: T) { items.0 }`), so the desugar
/// reproduces today's behaviour faithfully. A concrete pack type indexes
/// fine; a mapped pack is reached with a comprehension.
#[test]
fn positional_access_on_an_abstract_pack_is_refused_as_it_is_on_a_tuple() {
    assert_fails_with(
        r#"
        fun first<T: (2..)>(...items: T): i32 {
            items.0
        }
        fun main() {
            first(1, 2);
        }
        "#,
        "cannot access field '0' on type T",
    );
    assert_fails_with(
        r#"
        fun first<T: (2..)>(items: T): i32 {
            items.0
        }
        fun main() {
            first((1, 2));
        }
        "#,
        "cannot access field '0' on type T",
    );
}

/// The second PRE-EXISTING limit, likewise pinned beside its tuple-form twin: a
/// comprehension's source must be a MAPPED tuple, so a bare element-bounded
/// pack has no way to iterate its elements and the element bound has no
/// consumer of its own. Unchanged by the spread — the mapped form (`gather`
/// above) is what a comprehension reaches.
#[test]
fn a_comprehension_over_a_bare_pack_still_needs_a_mapped_source() {
    assert_fails_with(
        r#"
        import std::display::Display;
        fun render<T: (..: Display)>(...items: T): i32 {
            let rendered = (item in items => item.to_string());
            1
        }
        fun main() {
            render(1, 2);
        }
        "#,
        "a tuple comprehension's source must be a mapped tuple",
    );
    assert_fails_with(
        r#"
        import std::display::Display;
        fun render<T: (..: Display)>(items: T): i32 {
            let rendered = (item in items => item.to_string());
            1
        }
        fun main() {
            render((1, 2));
        }
        "#,
        "a tuple comprehension's source must be a mapped tuple",
    );
}

// --- Tuple-value spread (`(..a, b)`; backlog B3,
// --- proposal/variadic-generics.md §T). ONE type rule: a tuple construction's
// --- type is the CONCATENATION of its parts, a spread contributing its
// --- operand's elements. Every pin below reads off that rule.

/// The basic construction, in all four positions the rule admits (§T.3),
/// read back positionally.
#[test]
fn a_spread_concatenates_its_operands_elements() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pair = (1, 2);
            let lead = (..pair, 3);
            let trail = (0, ..pair);
            let mid = (0, ..pair, 9);
            let twice = (..pair, ..pair);
            print(lead.0);
            print(lead.2);
            print(trail.2);
            print(mid.3);
            print(twice.3);
        }
        "#,
        "1\n3\n2\n9\n2\n",
    );
}

/// §T.5 — emission reuses the tuple form's splice, so a spread produces the
/// SAME bytes as the nested element it replaces. The two differ only in type.
#[test]
fn a_spread_emits_the_flat_tuple_construction() {
    assert_emits_containing(
        r#"
        import std::print;
        fun main() {
            let inner = (10, 11);
            let flat = (..inner, 12);
            print(flat.0);
        }
        "#,
        "[ ...inner, 12 ]",
    );
}

/// The same bytes the NESTED element emits — the two constructions differ only
/// in type. Pinned as a pair so a divergence shows up here rather than as a
/// golden churn.
#[test]
fn a_spread_and_a_nested_element_emit_the_same_construction() {
    let nested = r#"
        import std::print;
        fun main() {
            let inner = (10, 11);
            let nested = (inner, 12);
            print(nested.0.0);
        }
        "#;
    assert_emits_containing(nested, "[ ...inner, 12 ]");
}

/// §T.3 — a construction whose ONLY entry is a spread is a tuple, not a group:
/// the ≥2 minimum exists only to keep `(e)` a grouping, and `..e` is not an
/// expression outside element position. Its type is the operand's, unchanged
/// (the concatenation of one).
#[test]
fn a_lone_spread_is_a_construction_not_a_group() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pair = (1, 2);
            let copy = (..pair);
            print(copy.0);
            print(copy.1);
        }
        "#,
        "1\n2\n",
    );
}

/// §T.3 — the EMPTY tuple concatenates like any other. `()` is an arity source
/// syntax cannot write as a value; §S made it reachable, and spreading one
/// contributes zero slots.
#[test]
fn an_empty_tuple_spreads_to_nothing() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun pack<T: (..)>(...items: T): T {
            items
        }
        fun main() {
            let none = pack();
            let one = (..none, 7);
            print(one.0);
        }
        "#,
        "7\n",
    );
}

/// §T.2 — the operand must be a tuple, and the refusal names the type.
#[test]
fn spreading_a_non_tuple_is_refused_naming_the_type() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            let n = 5;
            let t = (..n, 1);
            print(t.0);
        }
        "#,
        "cannot spread 'i32'",
    );
}

/// §T.2 — concatenation is ONE level: spreading a tuple whose own elements are
/// tuples keeps their nesting, because a spread contributes its operand's
/// elements, not its slots. This is the "types stay distinct" invariant under
/// flat storage — the runtime layout is identical, the type is not.
#[test]
fn concatenation_does_not_deep_flatten() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let inner = (1, 2);
            let outer = (inner, 3);
            let t = (..outer, 4);
            print(t.0.1);
            print(t.2);
        }
        "#,
        "2\n4\n",
    );
}

/// §T.2 — bidirectional inference survives the widening: the expected tuple is
/// sliced by the count of SLOTS produced so far, not by the entry's index, so
/// an annotation still reaches the entries after a spread (here typing an
/// unannotated closure's parameter).
#[test]
fn an_annotation_reaches_the_entries_after_a_spread() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pair = (1, 2);
            let t: (i32, i32, |i32| i32) = (..pair, |n| n + 1);
            print(t.2(9));
        }
        "#,
        "10\n",
    );
}

/// §T.2 — the concatenation is a tuple like any other, so it destructures.
#[test]
fn a_concatenation_destructures() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pair = (1, 2);
            let (a, b, c) = (..pair, 3);
            print(a + b + c);
        }
        "#,
        "6\n",
    );
}

/// §T.5 — the splice comes from the MARK, so an operand whose expression caches
/// no type of its own (a call, an `if`) still splices. This form was immune to
/// the §T.8 bug the nested form had, and stays immune now that the nested form
/// is fixed: it asks for no type lookup at all.
#[test]
fn a_spread_of_a_call_result_still_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make(): (i32, i32) {
            (4, 5)
        }
        fun main() {
            let t = (..make(), 6);
            print(t.0);
            print(t.2);
        }
        "#,
        "4\n6\n",
    );
}

/// The same, through an `if` — the other expression shape the type cache
/// misses.
#[test]
fn a_spread_of_a_conditional_still_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let t = (..if true { (1, 2) } else { (3, 4) }, 9);
            print(t.2);
        }
        "#,
        "9\n",
    );
}

/// §T.6 — `f(..pair, x)` mixes a spread with ordinary arguments at a spread
/// call site: the collection builds one tuple out of both.
#[test]
fn a_spread_call_site_mixes_spreads_and_ordinary_arguments() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun pack<T: (..)>(...items: T): T {
            items
        }
        fun main() {
            let pair = (1, 2);
            let trio = pack(..pair, 7);
            print(trio.2);
        }
        "#,
        "7\n",
    );
}

/// §T.6 — the bound is checked on the CONCATENATED arity, with the shipped
/// bound error, not a new one: nothing about bound checking changed.
#[test]
fn a_spread_argument_is_bound_checked_on_the_concatenation() {
    assert_fails_with(
        r#"
        import std::print;
        fun need3<T: (3..)>(...items: T): i32 {
            1
        }
        fun main() {
            let pair = (1, 2);
            print(need3(..pair));
        }
        "#,
        "'(i32, i32)' has 2 elements: the bound '(3..)' requires at least 3",
    );
}

/// §T.6 — a spread argument to a function with NO spread parameter builds no
/// tuple, so it is refused with the tuple form as the steer.
#[test]
fn a_spread_at_a_non_spread_call_is_refused_with_the_tuple_form() {
    assert_fails_with(
        r#"
        import std::print;
        fun forward(items: (i32, i32)): i32 {
            items.0
        }
        fun main() {
            let pair = (1, 2);
            print(forward(..pair));
        }
        "#,
        "Write `f((..pair))` to pass the concatenation as a single tuple argument",
    );
}

/// §T.6 — and at a CLOSURE call, which resolves down its own road. This is what
/// the one post-solve sweep buys over a check at each call path: a road the
/// collection never runs on still refuses.
#[test]
fn a_spread_at_a_closure_call_is_refused_too() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            let f = |items: (i32, i32)| items.0;
            let pair = (1, 2);
            print(f(..pair));
        }
        "#,
        "`..` splices a tuple's elements into a tuple construction",
    );
}

/// §T.4 — an ABSTRACT pack may be spread ALONE: the concatenation of one is
/// identity, so its type is `T` unchanged and no symbolic concatenation is
/// needed. This is the §S.4 payoff — forwarding a pack to another SPREAD
/// function, which `inner(items)` cannot do (it collects to `((T))`, §S.8).
#[test]
fn an_abstract_pack_may_be_spread_alone() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun inner<U: (..)>(...xs: U): i32 {
            1
        }
        fun outer<T: (..)>(...items: T): i32 {
            inner(..items)
        }
        fun main() {
            print(outer(1, 2, 3));
        }
        "#,
        "1\n",
    );
}

/// §T.4 — and is refused MIXED, with the reason: the body is checked once,
/// before any call fixes the arity, so there is no element sequence to
/// concatenate with. Not a carved exception — the lone case is well-typed
/// because there is nothing to concatenate, and this one is not because there
/// is.
#[test]
fn an_abstract_pack_may_not_be_concatenated_with_anything() {
    assert_fails_with(
        r#"
        fun inner<U: (..)>(...xs: U): i32 {
            1
        }
        fun outer<T: (..)>(...items: T): i32 {
            inner(..items, 9)
        }
        fun main() {
            outer(1, 2);
        }
        "#,
        "is a tuple of unknown arity here, so its elements cannot be concatenated",
    );
}

/// §T.2 — a MAPPED pack is a tuple once its source is concrete, so it is
/// expanded before the operand is judged and spreads like any other.
#[test]
fn a_mapped_pack_spreads_like_any_tuple() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::reactive::Signal;
        fun gather<T: (2..)>(...sources: (U in T: Signal<U>)): i32 {
            1
        }
        fun main() {
            let a = Signal::new(1);
            let b = Signal::new("x");
            let both = (a, b);
            print(gather(..both));
        }
        "#,
        "1\n",
    );
}

/// §T.1 — `...` written where a VALUE spread belongs gets its own steer, not a
/// `..` followed by a broken expression. The two markers are one dot apart and
/// one is the sibling feature.
#[test]
fn three_dots_in_a_value_position_steers_to_two() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            let pair = (1, 2);
            let t = (...pair, 3);
            print(t.0);
        }
        "#,
        "`...` marks a spread PARAMETER, on a declaration; a tuple-value spread is `..`",
    );
}

// --- B70 (§T.8): a tuple element splices by its TYPE, whatever FORM it is
// --- written in. The splice test read the general type cache, which stores a
// --- type only where one is *produced* — a binding, a literal, a projection, a
// --- match — so an element typed on demand (a call, an `if`, a block, an
// --- `await`, a `*view`, a bare parameter) read as untyped, nested instead of
// --- splicing, and every read past it came back `undefined`. The tuple rule
// --- now keeps the type it computes per element, so the coverage is by form
// --- and not by accident. One pin per form.

/// §T.8 — the filed repro, and the bug's root form. Was `#[ignore]`d;
/// reproduced on the released v0.28.0 binary and needs no `..`. The spread form
/// was never affected — `a_spread_of_a_call_result_still_splices` above is the
/// same program with a `..` — which is why §T.5 drives the spread's splice from
/// the mark.
#[test]
fn a_nested_tuple_element_that_is_a_call_should_still_splice() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make(): (i32, i32) {
            (4, 5)
        }
        fun main() {
            let n = (make(), 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — the other form the filed entry names. An `if` is an expression here,
/// and its value is the tuple its taken leg produced.
#[test]
fn a_nested_tuple_element_that_is_a_conditional_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pick = true;
            let n = (if pick { (4, 5) } else { (7, 8) }, 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — an `else if` chain is a nested `ExprIfBranch`, so the type is reached
/// through one more level than the two-leg form above.
#[test]
fn a_nested_tuple_element_that_is_an_else_if_chain_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let k = 2;
            let n = (if k == 1 { (4, 5) } else if k == 2 { (7, 8) } else { (9, 9) }, 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — a block's value is its trailing expression, and the block itself
/// stores no type.
#[test]
fn a_nested_tuple_element_that_is_a_block_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let n = ({ let inner = (4, 5); inner }, 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — a method call. Its own dispatch is a different path from a free
/// call's, and it stored no type either.
#[test]
fn a_nested_tuple_element_that_is_a_method_call_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Maker { seed: i32 }
        impl Maker {
            fun pair(self): (i32, i32) { (4, 5) }
        }
        fun main() {
            let maker = Maker { seed = 1 };
            let n = (maker.pair(), 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — an associated (static) call, reached through the type rather than a
/// receiver.
#[test]
fn a_nested_tuple_element_that_is_a_static_call_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Maker { seed: i32 }
        impl Maker {
            fun pair(): (i32, i32) { (4, 5) }
        }
        fun main() {
            let n = (Maker::pair(), 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — calling a CLOSURE-typed value: the callee is a binding, not a
/// function entity, so the return type comes from the closure type.
#[test]
fn a_nested_tuple_element_that_is_a_closure_call_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let make = || (4, 5);
            let n = (make(), 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — an `await`: the element's type is the awaited call's, unwrapped, and
/// the `await` node stores nothing of its own.
#[test]
fn a_nested_tuple_element_that_is_an_await_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        async fun make(): (i32, i32) {
            (4, 5)
        }
        async fun main() {
            let n = (await make(), 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — a read through a view. `*view` is a place expression whose type is
/// the pointee's, and it stored none.
#[test]
fn a_nested_tuple_element_that_is_a_dereference_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pair = (4, 5);
            let view = &pair;
            let n = (*view, 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — a bare PARAMETER. Its type lives on the parameter binding, and the
/// general cache's place fallback resolves a variable but not a parameter, so
/// this form read as untyped too. The one form that is a plain name and still
/// lost its splice.
#[test]
fn a_nested_tuple_element_that_is_a_parameter_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun wrap(pair: (i32, i32)): i32 {
            let n = (pair, 6);
            n.1
        }
        fun main() {
            print(wrap((4, 5)));
        }
        "#,
        "6\n",
    );
}

/// §T.8 — a `const`-marked element. Its value is folded at compile time, and
/// the folded result has to land in the same flat layout the type describes.
#[test]
fn a_nested_tuple_element_that_is_a_const_call_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make(): (i32, i32) {
            (4, 5)
        }
        fun main() {
            let n = (const make(), 6);
            print(n.1);
        }
        "#,
        "6\n",
    );
}

/// §T.8 — the forms that already stored a type keep splicing: a `match`, a
/// struct field, a list index, and a tuple projection. The fix consults the
/// element table only where the general cache is silent, so these must still
/// take the old path and answer the same.
#[test]
fn the_tuple_element_forms_that_already_stored_a_type_still_splice() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun main() {
            let k = 1;
            print((match k { 1 => (4, 5), _ => (7, 8) }, 6).1);
            let holder = Holder { pair = (4, 5) };
            print((holder.pair, 6).1);
            let pairs = [(4, 5), (7, 8)];
            print((pairs[0], 6).1);
            let nested = ((4, 5), 9);
            print((nested.0, 6).1);
        }
        "#,
        "6\n6\n6\n6\n",
    );
}

/// §T.8, mixed — one construction whose elements are a call, an `if`, a name and
/// a scalar, read at every offset. The widths only line up if each element
/// spliced or nested exactly as its own type says.
#[test]
fn a_construction_mixing_every_element_form_reads_at_every_offset() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make(): (i32, i32) {
            (1, 2)
        }
        fun main() {
            let named = (5, 6);
            let n = (0, make(), if true { (3, 4) } else { (9, 9) }, named, 7);
            print(n.0);
            print(n.1.1);
            print(n.2.0);
            print(n.3.1);
            print(n.4);
        }
        "#,
        "0\n2\n3\n6\n7\n",
    );
}

/// §T.8, nested — a call-valued element inside a construction that is ITSELF an
/// element. The inner construction's own type carries the flat widths, so the
/// outer offsets are only right if the inner splice happened.
#[test]
fn a_call_valued_element_of_a_nested_construction_splices() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make(): (i32, i32) {
            (1, 2)
        }
        fun main() {
            let n = ((make(), 3), 4);
            print(n.0.0.1);
            print(n.0.1);
            print(n.1);
        }
        "#,
        "2\n3\n4\n",
    );
}

/// §T.8, the spread twin — a construction holding BOTH a `..` element and a
/// call-valued one. The mark drives the first and the type drives the second;
/// they have to agree on the same flat layout.
#[test]
fn a_construction_holding_a_spread_and_a_call_element_splices_both() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make(): (i32, i32) {
            (3, 4)
        }
        fun main() {
            let head = (1, 2);
            let n = (..head, make(), 5);
            print(n.0);
            print(n.1);
            print(n.2.1);
            print(n.3);
        }
        "#,
        "1\n2\n4\n5\n",
    );
}

/// §T.8, the generic boundary — an element whose type is still a generic
/// parameter must NOT splice, even at an instantiation that binds it to a
/// tuple. A generic body is walked once and emitted per instantiation, and the
/// `.n` offsets baked into that single walk count a generic element as ONE slot
/// (`tuple_flat_width`); splicing it per instantiation would move every offset
/// past it. This is why the element table is read as written rather than
/// resolved through the active monomorphization.
#[test]
fn a_generic_valued_tuple_element_stays_nested_so_its_offsets_hold() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun wrap<T>(value: T, tail: i32): i32 {
            let n = (value, tail);
            n.1
        }
        fun main() {
            print(wrap((4, 5), 6));
            print(wrap(9, 6));
        }
        "#,
        "6\n6\n",
    );
}

// --- B70 tail (§T.8): a tuple's ARITY is part of its type. `reconcile_type`'s
// --- tuple arm zipped without a length check — unlike the array arm beside it,
// --- which unifies only at the same length, and the closure arm, which unifies
// --- only at the same parameter count — so it compared the common prefix and
// --- called `(i32, str)` a match for `(i32, str, bool)`, yielding a 2-tuple
// --- nobody wrote. Every position that reaches the reconciler accepted one
// --- silently. One pin per position.

/// §T.8 tail — an annotated binding. The plainest form: the write says three
/// slots and the value has two, and it compiled clean.
#[test]
fn an_annotated_binding_rejects_a_tuple_of_the_wrong_arity() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            let t: (i32, str, bool) = (1, "x");
            print(t.0);
        }
        "#,
        "Expected (i32, str, bool), but got (i32, str) instead.",
    );
}

/// §T.8 tail — an argument against a declared parameter type.
#[test]
fn an_argument_rejects_a_tuple_of_the_wrong_arity() {
    assert_fails_with(
        r#"
        import std::print;
        fun need(t: (i32, str, bool)) {
            print(t.0);
        }
        fun main() {
            need((1, "x"));
        }
        "#,
        "Expected (i32, str, bool), but got (i32, str) instead.",
    );
}

/// §T.8 tail — a body reconciled against its declared return type.
#[test]
fn a_return_rejects_a_tuple_of_the_wrong_arity() {
    assert_fails_with(
        r#"
        import std::print;
        fun make(): (i32, str, bool) {
            (1, "x")
        }
        fun main() {
            print(make().0);
        }
        "#,
        "Expected (i32, str, bool), but got (i32, str) instead.",
    );
}

/// §T.8 tail — an assignment to an already-typed binding.
#[test]
fn an_assignment_rejects_a_tuple_of_the_wrong_arity() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            mut t = (1, 2);
            t = (1, 2, 3);
            print(t.0);
        }
        "#,
        "Expected (i32, i32), but got (i32, i32, i32) instead.",
    );
}

/// §T.8 tail — two `match` legs reconciled against each other.
#[test]
fn match_legs_reject_tuples_of_different_arities() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            let k = 1;
            let t = match k { 1 => (1, 2), _ => (1, 2, 3) };
            print(t.0);
        }
        "#,
        "match legs have mismatched types",
    );
}

/// §T.8 tail — a list literal's elements, unified against the first one's type.
#[test]
fn a_list_literal_rejects_tuples_of_different_arities() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            let xs = [(1, 2), (1, 2, 3)];
            print(xs.len());
        }
        "#,
        "Expected (i32, i32) (this literal's element type), but got (i32, i32, i32) instead.",
    );
}

/// §T.8 tail — one generic parameter bound from two arguments. The reconciler is
/// what decides they are the same `T`, so a truncating zip made two different
/// tuple types agree.
#[test]
fn one_generic_bound_from_two_arguments_rejects_tuples_of_different_arities() {
    assert_fails_with(
        r#"
        import std::print;
        fun pick<T>(a: T, b: T): T {
            a
        }
        fun main() {
            let t = pick((1, 2), (1, 2, 3));
            print(t.0);
        }
        "#,
        "Expected (i32, i32), but got (i32, i32, i32) instead.",
    );
}

/// §T.8 tail — trait conformance (B29) compares through `compare_type_rigid`,
/// whose tuple arm zipped the same way. An impl returning a tuple of a different
/// arity than the trait declares was accepted.
#[test]
fn a_conformance_return_rejects_a_tuple_of_the_wrong_arity() {
    assert_fails_with(
        r#"
        import std::print;
        trait Pairs {
            fun make(self): (i32, i32);
        }
        struct S { n: i32 }
        impl S with Pairs {
            fun make(self): (i32, i32, i32) { (1, 2, 3) }
        }
        fun main() {
            let s = S { n = 1 };
            print(s.make().0);
        }
        "#,
        "`S`'s `make` returns `(i32, i32, i32)`, but `Pairs` declares `(i32, i32)`",
    );
}

/// §T.8 tail — the same through a conformance PARAMETER position, which is the
/// other `compare_type_rigid` call site.
#[test]
fn a_conformance_parameter_rejects_a_tuple_of_the_wrong_arity() {
    assert_fails_with(
        r#"
        import std::print;
        trait Takes {
            fun take(self, p: (i32, i32)): i32;
        }
        struct S { n: i32 }
        impl S with Takes {
            fun take(self, p: (i32, i32, i32)): i32 { p.0 }
        }
        fun main() {
            let s = S { n = 1 };
            print(s.take((1, 2, 3)));
        }
        "#,
        "parameter 1 of `S`'s `take` is `(i32, i32, i32)`, but `Takes` declares `(i32, i32)`",
    );
}

/// §T.8 tail — the arity check must not cost the MATCHING case anything: same
/// arity still unifies, still binds a generic element, and still runs.
#[test]
fn tuples_of_the_same_arity_still_reconcile() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun pick<T>(a: T, b: T): T {
            b
        }
        fun main() {
            let t: (i32, str) = (1, "x");
            print(t.0);
            let picked = pick((1, 2), (3, 4));
            print(picked.1);
        }
        "#,
        "1\n4\n",
    );
}

// --- B71: a nested free `fun` is emitted ONCE. Emission is demand-driven from
// --- the roots — a call emits the callee at module level, keyed on its id — but
// --- the body walk ALSO emitted a `fun` declaration inline where it was
// --- written, so a nested one came out twice with identical bodies, the inner
// --- shadowing the outer. Harmless at runtime, which is why it went unseen; it
// --- is dead output no reader of the JS can account for, and any change to
// --- which copy a call resolves to would make it live. A count, not a run: the
// --- duplicate is invisible to `assert_compiles_and_runs`.

/// B71 — the filed shape: a nested `fun` inside an IMPL METHOD's body.
#[test]
fn a_nested_fun_inside_an_impl_method_emits_once() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::print;
            struct S { n: i32 }
            impl S {
                fun run(self): i32 {
                    fun helper(x: i32): i32 { x + 4242 }
                    helper(self.n)
                }
            }
            fun main() {
                let s = S { n = 1 };
                print(s.run());
            }
            "#,
            "return x + 4242;",
        ),
        1,
    );
}

/// B71 — the same in a plain function's body. The filed entry named the impl
/// method; the double visit is the item walk's and has nothing to do with
/// `impl`, so a free function's nested `fun` doubled identically.
#[test]
fn a_nested_fun_inside_a_free_function_emits_once() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::print;
            fun run(n: i32): i32 {
                fun helper(x: i32): i32 { x + 4242 }
                helper(n)
            }
            fun main() {
                print(run(1));
            }
            "#,
            "return x + 4242;",
        ),
        1,
    );
}

/// B71 — two nested `fun`s, one calling the other. Each emits once, and the
/// inner call still resolves.
#[test]
fn two_nested_funs_each_emit_once() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::print;
            fun run(n: i32): i32 {
                fun first(x: i32): i32 { x + 4242 }
                fun second(x: i32): i32 { first(x) + 1 }
                second(n)
            }
            fun main() {
                print(run(1));
            }
            "#,
            "return x + 4242;",
        ),
        1,
    );
}

/// B71 — an uncalled nested `fun` emits NOTHING, which is what demand-driven
/// emission means and what the inline copy was quietly overriding: it was
/// emitted for having been written, not for being reached.
#[test]
fn an_uncalled_nested_fun_emits_nothing() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::print;
            fun run(n: i32): i32 {
                fun helper(x: i32): i32 { x + 4242 }
                n
            }
            fun main() {
                print(run(1));
            }
            "#,
            "return x + 4242;",
        ),
        0,
    );
}

/// B71 — a nested `fun` that SHADOWS a module-level one of the same name. Both
/// emit, once each, under distinct generated names, and each call reaches the
/// one its scope names.
#[test]
fn a_nested_fun_shadowing_a_module_level_one_keeps_both_calls_right() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun helper(x: i32): i32 { x + 100 }
        fun run(n: i32): i32 {
            fun helper(x: i32): i32 { x + 1 }
            helper(n)
        }
        fun main() {
            print(run(1));
            print(helper(1));
        }
        "#,
        "2\n101\n",
    );
}

// --- J2 value-flow asyncness: the marker on fields and return types,
// --- adoption for unannotated bindings, and the divergence refusals
// --- (backlog J2 "REMAINING" channels — closing the static-type/runtime-
// --- value split for closures that reach a call through a value flow).

#[test]
fn an_unannotated_binding_adopts_its_async_closure() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun main() {
            let f = || {
                sleep(1);
                1
            };
            print(f());
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_mut_rebind_adopts_asyncness() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun main() {
            mut f = || 1;
            f = || {
                sleep(1);
                3
            };
            print(f());
        }
        "#,
        "3\n",
    );
}

#[test]
fn an_async_field_call_awaits() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        struct Holder {
            handler: async || i32,
        }
        fun main() {
            let holder = Holder { handler = || {
                sleep(1);
                2
            } };
            print((holder.handler)());
            let taken = holder.handler;
            print(taken());
        }
        "#,
        "2\n2\n",
    );
}

#[test]
fn an_async_returning_call_awaits_directly_and_through_a_binding() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun make(): async || i32 {
            || {
                sleep(1);
                7
            }
        }
        fun main() {
            print(make()());
            let g = make();
            print(g());
        }
        "#,
        "7\n7\n",
    );
}

#[test]
fn an_async_closure_into_a_plain_field_is_refused() {
    assert_fails_with(
        r#"
        import std::time::sleep;
        struct Plain {
            h: || i32,
        }
        fun main() {
            let p = Plain { h = || {
                sleep(1);
                2
            } };
        }
        "#,
        "field `h` of `Plain` receives an async closure, but its type awaits nothing",
    );
}

#[test]
fn an_async_closure_assigned_into_a_plain_field_is_refused() {
    assert_fails_with(
        r#"
        import std::time::sleep;
        struct Plain {
            h: || i32,
        }
        fun main() {
            mut p = Plain { h = || 1 };
            p.h = || {
                sleep(1);
                9
            };
        }
        "#,
        "field `h` of `Plain` receives an async closure",
    );
}

#[test]
fn a_plain_declared_return_of_an_async_closure_is_refused() {
    assert_fails_with(
        r#"
        import std::time::sleep;
        fun bad(): || i32 {
            || {
                sleep(1);
                1
            }
        }
        fun main() {
            bad();
        }
        "#,
        "`bad` returns an async closure, but its declared return type awaits nothing",
    );
}

// Spawn-semantics parity: a VOID-returning async closure may flow into a
// plain void field — nothing is lied about, matching the parameter rule.
#[test]
fn a_void_async_closure_into_a_plain_void_field_stays_legal() {
    assert_compiles(
        r#"
        import std::print;
        import std::time::sleep;
        struct Plain {
            run: || ,
        }
        fun main() {
            let p = Plain { run = || {
                sleep(1);
                print("later");
            } };
        }
        "#,
    );
}

// The stray-position message names every supported position.
#[test]
fn a_stray_async_marker_names_the_supported_positions() {
    assert_fails_with(
        r#"
        fun main() {
            let xs: List<async || i32> = List::new();
        }
        "#,
        "only supported on parameters, `let` annotations, struct fields, and function return types",
    );
}

// --- The `x.field()` steers: method lookup does not fall back to fields,
// --- so a same-named field redirects to the right syntax (user request
// --- 2026-07-17; diagnostics-standard B4).

#[test]
fn a_closure_field_called_as_a_method_steers_to_parens() {
    assert_fails_with(
        r#"
        struct Holder {
            handler: || i32,
        }
        fun main() {
            let holder = Holder { handler = || 1 };
            let a = holder.handler();
        }
        "#,
        "parenthesize the field access to call it, `(x.handler)()`",
    );
}

#[test]
fn a_non_closure_field_called_as_a_method_steers_to_plain_access() {
    assert_fails_with(
        r#"
        struct Holder {
            count: i32,
        }
        fun main() {
            let holder = Holder { count = 3 };
            let b = holder.count();
        }
        "#,
        "`count` is a field of type `i32`, which is not callable: did you mean the plain access `x.count`?",
    );
}

#[test]
fn a_true_method_miss_keeps_the_bare_message() {
    let source = r#"
        struct Holder {
            count: i32,
        }
        fun main() {
            let holder = Holder { count = 3 };
            holder.missing();
        }
        "#;
    assert_fails_with(source, "Holder has no method 'missing'");
    match compile(source) {
        Ok(_) => panic!("expected a compile error"),
        Err(errors) => assert!(
            errors.iter().all(|error| !error.contains("field")),
            "no field steer should fire without a same-named field: {errors:#?}"
        ),
    }
}

// --- The `sync` closure contract (proposal/async-polymorphism.md A.2):
// --- a contextual marker on parameters — async arguments are refused with
// --- the contract steer; plain names stay legal.

#[test]
fn a_sync_parameter_accepts_a_sync_closure_and_runs() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun run_now(body: sync || i32): i32 {
            body()
        }
        fun main() {
            print(run_now(|| 5));
        }
        "#,
        "5\n",
    );
}

#[test]
fn a_sync_parameter_refuses_an_async_closure() {
    assert_fails_with(
        r#"
        import std::time::sleep;
        fun run_now(body: sync || i32): i32 {
            body()
        }
        fun main() {
            run_now(|| {
                sleep(1);
                1
            });
        }
        "#,
        "requires a synchronous closure (`sync`): its completion is part of the declaring function's synchronous protocol",
    );
}

#[test]
fn a_stray_sync_marker_is_rejected() {
    assert_fails_with(
        r#"
        fun main() {
            let x: sync || i32 = || 1;
        }
        "#,
        "a `sync` closure contract is only supported on parameters",
    );
}

// `sync` is contextual: types and values named `sync` stay legal.
#[test]
fn sync_stays_a_legal_name() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct sync {
            n: i32,
        }
        fun main() {
            let named: sync = sync { n = 2 };
            print(named.n);
        }
        "#,
        "2\n",
    );
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let sync = 9;
            print(sync);
        }
        "#,
        "9\n",
    );
}

// --- Adaptation (proposal/async-polymorphism.md A.1): plain value-returning
// --- closure parameters are asyncness-polymorphic — an async argument
// --- instantiates an ASYNC instance of the callee (calls through the
// --- parameter await, sequentially); sync call sites are untouched.

#[test]
fn an_async_closure_adapts_map_and_runs_sequentially() {
    // The callbacks' side effects land in SOURCE ORDER (the sequential
    // contract), and the mapped values are settled — not promises.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun main() {
            let urls = ["ab", "cdef"];
            let ids = urls.map(|url| {
                let length = url.len();
                sleep(1);
                print(length);
                length
            });
            print(ids);
        }
        "#,
        "2\n4\n[ 2, 4 ]\n",
    );
}

#[test]
fn a_non_generic_function_adapts() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun run(f: || i32): i32 {
            f() + 100
        }
        fun main() {
            print(run(|| {
                sleep(1);
                7
            }));
            print(run(|| 1));
        }
        "#,
        "107\n101\n",
    );
}

#[test]
fn adaptation_rides_through_a_forwarding_helper() {
    // Transitive: helper's plain parameter forwards into map — helper and
    // map both instantiate adapted, and the caller awaits the chain.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun helper(urls: List<str>, f: |str| i32): List<i32> {
            urls.map(f)
        }
        fun main() {
            print(helper(["ab", "cdef"], |url| {
                sleep(1);
                url.len() + 10
            }));
        }
        "#,
        "[ 12, 14 ]\n",
    );
}

#[test]
fn a_forwarded_async_closure_into_a_sync_contract_is_refused() {
    assert_fails_noting(
        r#"
        import std::time::sleep;
        fun run_sync(g: sync || i32): i32 {
            g()
        }
        fun forwards(f: || i32): i32 {
            run_sync(f)
        }
        fun main() {
            forwards(|| {
                sleep(1);
                2
            });
        }
        "#,
        "passes an async closure that reaches `g`, which requires a synchronous closure (`sync`)",
        "run_sync(f)",
        "forwarded into the `sync` parameter `g` here",
    );
}

#[test]
fn an_async_closure_into_an_extern_callback_is_refused() {
    assert_fails_with(
        r#"
        import std::time::sleep;
        external fun host_transform(f: |i32| i32): i32;
        fun main() {
            host_transform(|n| {
                sleep(1);
                n
            });
        }
        "#,
        "`host_transform` is a host (`external`) function: it cannot await a Vilan closure",
    );
}

/// E68 — the coverage-error cascade. Any `owner_scope` coverage failure makes
/// `thread_contexts` refuse its rewrite, leaving `Context::run` calls visible
/// to the host-boundary check, which then judged std's own async-into-`run`
/// bodies (task.vl's nursery, rpc.vl's wire turn) as host-await misuses — a
/// false secondary anchored in std beside the primary. `run` is `external`
/// only as a type-checking fiction (the threading pass erases every call it
/// accepts), so it is never a host boundary; only the primary reports.
#[test]
fn e68_an_uncovered_effect_reports_only_the_coverage_primary() {
    let source = r#"
        import std::print;
        import std::reactive::Signal;

        fun main() {
            let counter = Signal::new(0);
            counter.effect(|value| print(value));
        }
        "#;
    assert_fails_with(
        source,
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_fails_without(source, "cannot await a Vilan closure");
}

/// E68's second probe shape: a closure VALUE passed to `run` trips the
/// `run`-shape rule (and the coverage fence for the closure's own reads);
/// the refused rewrite must not surface the host-await secondary either.
#[test]
fn e68_a_refused_run_shape_reports_only_the_context_primaries() {
    let source = r#"
        import std::print;
        import std::reactive::{ Owner, Signal, owner_scope };

        fun main() {
            let counter = Signal::new(0);
            let body = || {
                counter.effect(|value| print(value));
            };
            owner_scope.run(Owner::new(), body);
        }
        "#;
    assert_fails_with(
        source,
        "`run` must be called on a named context with a closure literal body",
    );
    assert_fails_without(source, "cannot await a Vilan closure");
}

/// E68's transitive arm: an async closure forwarded through a generic into
/// `run` used to trip `extern_violations_at` too ("reaches the host
/// (`external`) function `run`") — same false premise, same exemption. The
/// `assert_fails_without` fragment appears in both the direct and the
/// transitive spurious message, so this pin holds both arms shut.
#[test]
fn e68_a_generic_forward_into_run_does_not_cascade_transitively() {
    let source = r#"
        import std::reactive::{ Owner, owner_scope };
        import std::time::sleep;

        fun helper<T>(body: || T): T {
            owner_scope.run(Owner::new(), body)
        }

        fun main() {
            let value = helper(|| {
                sleep(1);
                2
            });
        }
        "#;
    assert_fails_with(
        source,
        "`run` must be called on a named context with a closure literal body",
    );
    assert_fails_without(source, "cannot await a Vilan closure");
}

#[test]
fn adaptation_cannot_ride_a_trait_dispatch() {
    assert_fails_with(
        r#"
        import std::time::sleep;
        trait Runner {
            fun run_with(self, f: || i32): i32;
        }
        struct Fast {}
        impl Fast with Runner {
            fun run_with(self, f: || i32): i32 {
                f()
            }
        }
        fun go<R: Runner>(runner: R): i32 {
            runner.run_with(|| {
                sleep(1);
                1
            })
        }
        fun main() {
            go(Fast {});
        }
        "#,
        "an async closure cannot adapt a trait/generic-dispatched call",
    );
}

#[test]
fn a_module_initializer_cannot_adapt_await() {
    assert_fails_with(
        r#"
        import std::print;
        import std::time::sleep;
        let ids = ["ab"].map(|s| {
            sleep(1);
            s.len()
        });
        fun main() {
            print(ids);
        }
        "#,
        "a module-level binding cannot await",
    );
}

// --- The Task<T> substrate (proposal/async-polymorphism.md Part B): `async e`
// --- yields a `Task<T>` handle — eager, absorbed-at-construction, copy =
// --- same task. `await` unwraps a Task or a raw host Promise.

#[test]
fn a_spawn_types_as_task_and_await_unwraps_it() {
    assert_compiles(
        r#"
        import std::print;
        import std::task::Task;
        fun label(): str { "ready" }
        fun main() {
            let t: Task<str> = async label();
            let s: str = await t;
            print(s);
        }
        "#,
    );
}

#[test]
fn a_task_is_not_a_promise() {
    // The raw host-interop promise and the spawn handle are distinct types.
    assert_fails_with(
        r#"
        import std::task::Task;
        import std::promise::Promise;
        fun label(): str { "ready" }
        fun main() {
            let p: Promise<str> = async label();
            let _ = await p;
        }
        "#,
        "Expected Promise<str>, but got Task<str>",
    );
}

#[test]
fn spawn_typing_falls_back_to_promise_without_std_task() {
    // Compat: a program that loads `std::promise` but never `std::task`
    // keeps the old `Promise<T>` spawn typing (an older std has no task.vl).
    assert_compiles(
        r#"
        import std::print;
        import std::promise::Promise;
        fun label(): str { "ready" }
        fun main() {
            let p: Promise<str> = async label();
            print(await p);
        }
        "#,
    );
}

#[test]
fn a_raw_host_promise_still_types_and_awaits() {
    // `[extern(new, "Promise")]` — the host-interop seam stays `Promise<T>`,
    // and `await` unwraps it exactly like a task.
    assert_compiles(
        r#"
        import std::print;
        import std::promise::Promise;
        import std::task::Task;
        [extern(new, "Promise")]
        external fun ticket(executor: |(|i32| void)| void): Promise<i32>;
        fun main() {
            let p: Promise<i32> = ticket(|resolve| { resolve(7); });
            let n: i32 = await p;
            print(n);
        }
        "#,
    );
}

#[test]
fn settle_all_preserves_order() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::Task;
        fun delayed(label: str, ms: i32): str {
            sleep(ms);
            label
        }
        fun main() {
            mut tasks: List<Task<str>> = List::new();
            tasks.push(async delayed("a", 20));
            tasks.push(async delayed("b", 10));
            tasks.push(async delayed("c", 30));
            let results: List<str> = Task::settle_all(tasks);
            for result in results {
                print(result);
            }
        }
        "#,
        "a\nb\nc\n",
    );
}

#[test]
fn a_task_is_a_handle_copies_observe_the_same_run() {
    // Copying the handle refers to the SAME task: the body runs once, and
    // both copies observe its (single) result.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun work(): i32 {
            sleep(1);
            print("ran");
            7
        }
        fun main() {
            let t = async work();
            let copy = t;
            print(await copy);
            print(await t);
        }
        "#,
        "ran\n7\n7\n",
    );
}

#[test]
fn an_unobserved_task_failure_reports_and_the_program_continues() {
    // Absorption: the failed spawn never becomes a host unhandled rejection
    // (which would crash node). One macrotask after it settles unobserved,
    // it is reported to stderr with the spawn origin — and main still runs
    // to completion with exit 0.
    match compile_and_run_capturing_stderr(
        r#"
        import std::print;
        import std::io::panic;
        import std::time::sleep;
        fun doomed(): i32 {
            panic("boom")
        }
        fun main() {
            let _ = async doomed();
            sleep(10);
            print("alive");
        }
        "#,
    ) {
        Ok((stdout, stderr)) => {
            assert_eq!(stdout, "alive\n", "stdout mismatch");
            assert!(
                stderr.contains("unhandled task error (spawned in main): boom"),
                "missing the origin-stamped report, stderr was: {stderr:?}"
            );
        }
        Err(errors) => panic!("expected a clean (exit 0) run, got: {errors:#?}"),
    }
}

#[test]
fn a_promptly_awaited_failure_delivers_without_a_report() {
    // The awaiting side receives the panic (the process fails with it), and
    // no unobserved-failure report fires for an observed task.
    match compile_and_run(
        r#"
        import std::print;
        import std::io::panic;
        fun doomed(): i32 {
            panic("boom")
        }
        fun main() {
            let t = async doomed();
            print(await t);
        }
        "#,
    ) {
        Ok(stdout) => panic!("expected the run to fail with the panic, got: {stdout:?}"),
        Err(errors) => {
            let stderr = errors.join("\n");
            assert!(stderr.contains("boom"), "stderr was: {stderr:?}");
            assert!(
                !stderr.contains("unhandled task error"),
                "an observed task must not also report, stderr was: {stderr:?}"
            );
        }
    }
}

#[test]
fn a_late_await_still_receives_an_absorbed_failure() {
    // Absorption is not loss: even after the unobserved report has fired,
    // awaiting the task delivers the original failure.
    match compile_and_run(
        r#"
        import std::print;
        import std::io::panic;
        import std::time::sleep;
        fun doomed(): i32 {
            panic("boom")
        }
        fun main() {
            let t = async doomed();
            sleep(10);
            print(await t);
        }
        "#,
    ) {
        Ok(stdout) => panic!("expected the run to fail with the panic, got: {stdout:?}"),
        Err(errors) => {
            let stderr = errors.join("\n");
            assert!(stderr.contains("boom"), "stderr was: {stderr:?}");
        }
    }
}

// --- Nurseries (proposal/async-polymorphism.md Part B): `nursery(body)` joins
// --- every task spawned in its dynamic extent; failures follow the
// --- first-observed rule with absorption; the extent rides the context pass.

#[test]
fn nursery_returns_its_body_value_after_joining() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        fun main() {
            let value = nursery(|n| {
                let _ = async {
                    sleep(20);
                    print("child");
                };
                print("body");
                7
            });
            print(value);
            print("after");
        }
        "#,
        "body\nchild\n7\nafter\n",
    );
}

#[test]
fn nursery_extent_reaches_helpers_and_grandchildren() {
    // Dynamic extent: a helper CALLED from the body spawns into the nursery
    // (no plumbing), and a task spawned by a running child (a grandchild,
    // registered while the join is already draining) is joined too.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        fun spawn_step(label: str, ms: i32) {
            let _ = async {
                sleep(ms);
                print(label);
            };
        }
        fun main() {
            nursery(|n| {
                spawn_step("helper-spawned", 15);
                let _ = async {
                    sleep(5);
                    spawn_step("grandchild", 20);
                    print("child");
                };
                0
            });
            print("joined");
        }
        "#,
        "child\nhelper-spawned\ngrandchild\njoined\n",
    );
}

#[test]
fn a_spawn_outside_the_nursery_extent_stays_free_floating() {
    // The SAME helper registers when called inside the extent and stays
    // free-floating outside it (the safe flavor's absent value): "inside"
    // is joined before the nursery returns and prints BEFORE "mid";
    // "outside" is not joined by anything, so it floats past "end" and only
    // prints when its own timer fires.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        fun work(label: str) {
            let _ = async {
                sleep(10);
                print(label);
            };
        }
        fun main() {
            nursery(|n| {
                work("inside");
                0
            });
            print("mid");
            work("outside");
            print("end");
        }
        "#,
        "inside\nmid\nend\noutside\n",
    );
}

#[test]
fn a_body_throw_wins_and_children_absorb_silently() {
    match compile_and_run(
        r#"
        import std::print;
        import std::io::panic;
        import std::time::sleep;
        import std::task::nursery;
        fun main() {
            nursery(|n| {
                let _ = async {
                    sleep(30);
                    panic("late-child")
                };
                panic("body-first")
            });
            print("unreachable");
        }
        "#,
    ) {
        Ok(stdout) => panic!("expected the nursery failure to propagate, got: {stdout:?}"),
        Err(errors) => {
            let stderr = errors.join("\n");
            assert!(stderr.contains("body-first"), "stderr was: {stderr:?}");
            assert!(
                !stderr.contains("late-child"),
                "the losing child must be absorbed silently, stderr was: {stderr:?}"
            );
            assert!(
                !stderr.contains("unhandled task error"),
                "absorbed children must not default-report, stderr was: {stderr:?}"
            );
        }
    }
}

#[test]
fn the_earliest_settled_child_failure_wins_with_origin() {
    match compile_and_run(
        r#"
        import std::print;
        import std::io::panic;
        import std::time::sleep;
        import std::task::nursery;
        fun fail_after(ms: i32, message: str): i32 {
            sleep(ms);
            panic(message)
        }
        fun main() {
            nursery(|n| {
                let _ = async fail_after(25, "slow-loser");
                let _ = async fail_after(5, "fast-winner");
                0
            });
            print("unreachable");
        }
        "#,
    ) {
        Ok(stdout) => panic!("expected the nursery failure to propagate, got: {stdout:?}"),
        Err(errors) => {
            let stderr = errors.join("\n");
            assert!(
                stderr.contains("fast-winner (in task spawned in main)"),
                "the earliest-settled failure wins, origin-stamped; stderr was: {stderr:?}"
            );
            assert!(
                !stderr.contains("slow-loser"),
                "the later failure must be absorbed silently, stderr was: {stderr:?}"
            );
            assert!(
                !stderr.contains("unhandled task error"),
                "nursery-owned tasks must never default-report, stderr was: {stderr:?}"
            );
        }
    }
}

#[test]
fn nested_nurseries_join_inside_out() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        fun main() {
            let total = nursery(|outer| {
                let _ = async {
                    sleep(25);
                    print("outer-child");
                };
                let inner_value = nursery(|inner| {
                    let _ = async {
                        sleep(10);
                        print("inner-child");
                    };
                    print("inner-body");
                    2
                });
                print("inner-done");
                inner_value + 1
            });
            print(total);
        }
        "#,
        "inner-body\ninner-child\ninner-done\nouter-child\n3\n",
    );
}

#[test]
fn an_async_nursery_body_adapts() {
    // The body parameter is a plain closure parameter, so an awaiting body
    // rides adaptation (Part A) into the nursery machinery.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        fun main() {
            let v = nursery(|n| {
                sleep(5);
                let _ = async {
                    sleep(10);
                    print("child");
                };
                print("async-body");
                9
            });
            print(v);
        }
        "#,
        "async-body\nchild\n9\n",
    );
}

#[test]
fn spawn_then_settle_composes_with_a_nursery() {
    // `settle_all` observes the tasks first; the join then re-awaits the
    // already-settled children instantly. Both idioms coexist.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::{ nursery, Task };
        fun delayed(value: i32): i32 {
            sleep(5);
            value * 10
        }
        fun main() {
            let results = nursery(|n| {
                let tasks = [1, 2, 3].map(|value| async delayed(value));
                Task::settle_all(tasks)
            });
            print(results);
        }
        "#,
        "[ 10, 20, 30 ]\n",
    );
}

// --- Cancellation (Part B slice 3): n.cancel(), the AbortSignal bridge into
// --- std IO (sleep/fetch carry the ambient signal), settle-time failure
// --- reaction, nested chaining, and the race idiom.

#[test]
fn cancel_cuts_a_sleeping_child_short_and_keeps_the_value() {
    // The child's 5000ms sleep aborts when the body cancels; its AbortError
    // is a cancellation echo (absorbed, not a winner) and the body's value
    // comes back. The elapsed bound is what pins the abort — without it the
    // join would wait out the timer. Only the RUN is timed (E32): compiling
    // this program re-analyzes `std` in-process and can itself take
    // seconds under nextest's full parallelism, which is not part of the
    // claim (the claim is about the emitted program's own scheduling, not
    // the harness's compile step).
    assert_runs_within(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        fun main() {
            let v = nursery(|n| {
                let _ = async {
                    sleep(5000);
                    print("never");
                };
                sleep(30);
                n.cancel();
                print("cancelled");
                1
            });
            print(v);
        }
        "#,
        "cancelled\n1\n",
        std::time::Duration::from_secs(4),
    );
}

#[test]
fn a_fast_failure_behind_a_slow_sibling_reacts_at_settle_time() {
    // children[0] sleeps 5000ms; children[1] fails at 20ms. The failure
    // latches AT SETTLE (not at drain order), aborts the sibling's sleep,
    // and wins with its origin — promptly. Only the RUN is timed (E32):
    // `compile_and_run_timed` runs the in-process `std` re-analysis first,
    // untimed, so the budget below measures the emitted program's own
    // reaction time, not a harness compile step that can itself take
    // seconds under nextest's full parallelism.
    let (outcome, elapsed) = compile_and_run_timed(
        r#"
        import std::print;
        import std::io::panic;
        import std::time::sleep;
        import std::task::nursery;
        fun main() {
            nursery(|n| {
                let _ = async {
                    sleep(5000);
                    print("never-b");
                };
                let _ = async {
                    sleep(20);
                    panic("boom-a")
                };
                0
            });
            print("unreachable");
        }
        "#,
    );
    match outcome {
        Ok(stdout) => panic!("expected the nursery failure to propagate, got: {stdout:?}"),
        Err(errors) => {
            let stderr = errors.join("\n");
            assert!(
                stderr.contains("boom-a (in task spawned in main)"),
                "stderr was: {stderr:?}"
            );
            assert!(!stderr.contains("never-b"), "stderr was: {stderr:?}");
        }
    }
    assert!(
        elapsed < std::time::Duration::from_secs(4),
        "the first error should abort the slow sibling, not wait it out (run alone took {elapsed:?}, compile excluded)"
    );
}

#[test]
fn outer_cancel_chains_into_nested_nurseries() {
    // The inner nursery chains to the outer's signal at creation: the outer
    // cancel aborts the inner's sleeping child, the echo absorbs, and the
    // inner nursery still returns its value. Only the RUN is timed (E32):
    // see `cancel_cuts_a_sleeping_child_short_and_keeps_the_value` above for
    // why the compile step is excluded from the budget.
    assert_runs_within(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        fun main() {
            nursery(|outer| {
                let _ = async {
                    sleep(20);
                    outer.cancel();
                };
                let v = nursery(|inner| {
                    let _ = async {
                        sleep(5000);
                        print("never");
                    };
                    3
                });
                print("inner-returned");
                print(v);
                0
            });
            print("done");
        }
        "#,
        "inner-returned\n3\ndone\n",
        std::time::Duration::from_secs(4),
    );
}

#[test]
fn is_cancelled_reads_and_an_explicit_cancel_keeps_the_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::nursery;
        fun main() {
            let v = nursery(|n| {
                print(n.is_cancelled());
                n.cancel();
                print(n.is_cancelled());
                5
            });
            print(v);
        }
        "#,
        "false\ntrue\n5\n",
    );
}

#[test]
fn the_race_idiom_yields_the_first_settled_and_aborts_the_losers() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::{ nursery, Task };
        fun main() {
            let winner = nursery(|n| {
                let a = async {
                    sleep(300);
                    "slow"
                };
                let b = async {
                    sleep(10);
                    "fast"
                };
                let w = Task::race([a, b]);
                n.cancel();
                w
            });
            print(winner);
        }
        "#,
        "fast\n",
    );
}

#[test]
fn a_module_initializer_cannot_run_a_nursery() {
    assert_fails_with(
        r#"
        import std::print;
        import std::time::sleep;
        import std::task::nursery;
        let banner = nursery(|n| {
            sleep(1);
            "ready"
        });
        fun main() {
            print(banner);
        }
        "#,
        "the initializer of `banner` calls `nursery`, which is async",
    );
}

#[test]
fn a_module_initializer_cannot_run_an_awaiting_context_body() {
    // The lowered `run(value, body)` is a directly-applied closure — the J3
    // check names the shape instead of a function.
    assert_fails_with(
        r#"
        import std::print;
        import std::time::sleep;
        import std::context::Context;
        let flavor: Context<i32> = Context::new();
        let banner = flavor.run(7, || {
            sleep(1);
            "ready"
        });
        fun main() {
            print(banner);
        }
        "#,
        "the initializer of `banner` runs a closure that awaits",
    );
}

// --- J2 laundering (the divergence channels on the full VALUE oracle): an
// --- async value reaches a plain field / sync contract / host callback /
// --- declared return through ANY channel — a declared parameter, a field
// --- read, a returning call — not just a held literal.

#[test]
fn an_async_parameter_cannot_launder_into_a_plain_field() {
    // The http.vl shape: a declared-async parameter stored into a plain
    // value-returning closure field escaped the old literal-only check.
    assert_fails_with(
        r#"
        struct Holder {
            hook: |i32| i32,
        }
        fun install(f: async |i32| i32): Holder {
            Holder { hook = f }
        }
        fun main() {
            let _ = install(|n| n + 1);
        }
        "#,
        "field `hook` of `Holder` receives an async closure",
    );
}

#[test]
fn an_async_field_read_cannot_launder_into_a_plain_field() {
    assert_fails_with(
        r#"
        struct A {
            hook: async |i32| i32,
        }
        struct B {
            hook: |i32| i32,
        }
        fun copy(a: A): B {
            B { hook = a.hook }
        }
        fun main() {
            let _ = copy(A { hook = |n| n });
        }
        "#,
        "field `hook` of `B` receives an async closure",
    );
}

#[test]
fn an_async_returning_call_cannot_launder_into_a_plain_field() {
    assert_fails_with(
        r#"
        import std::time::sleep;
        struct Holder {
            hook: || i32,
        }
        fun make(): async || i32 {
            || {
                sleep(1);
                2
            }
        }
        fun main() {
            let _ = Holder { hook = make() };
        }
        "#,
        "field `hook` of `Holder` receives an async closure",
    );
}

#[test]
fn an_async_parameter_cannot_launder_into_a_sync_contract() {
    assert_fails_with(
        r#"
        fun apply(f: sync |i32| i32): i32 {
            f(2)
        }
        fun outer(f: async |i32| i32): i32 {
            apply(f)
        }
        fun main() {
            let _ = outer(|n| n + 1);
        }
        "#,
        "requires a synchronous closure",
    );
}

#[test]
fn an_async_parameter_cannot_launder_into_a_host_callback() {
    assert_fails_with(
        r#"
        [extern("hostApply")]
        external fun host_apply(f: |i32| i32): i32;
        fun outer(f: async |i32| i32): i32 {
            host_apply(f)
        }
        fun main() {
            let _ = outer(|n| n + 1);
        }
        "#,
        "cannot await a Vilan closure",
    );
}

#[test]
fn an_async_parameter_cannot_launder_through_a_declared_return() {
    assert_fails_with(
        r#"
        fun make(f: async |i32| i32): |i32| i32 {
            f
        }
        fun main() {
            let _ = make(|n| n + 1);
        }
        "#,
        "returns an async closure, but its declared return type awaits nothing",
    );
}

#[test]
fn a_void_async_parameter_still_stores_as_spawn() {
    // Void positions keep spawn semantics at every boundary — storing a
    // void-returning async handler in a plain void field stays legal.
    assert_compiles(
        r#"
        struct Holder {
            on_done: |i32| void,
        }
        fun install(f: async |i32| void): Holder {
            Holder { on_done = f }
        }
        fun main() {
            let _ = install(|n| {});
        }
        "#,
    );
}

// --- The adapted-instance escape (async-polymorphism.md A.4) ----------------
//
// A.4 recorded that adaptation covers the closures a body CALLS, and asserted
// that a body which stores or returns a parameter closure instead "uses the
// existing rules (the field/return divergence checks catch lies)". It did not:
// those checks run outside any instance context, so they only ever saw the
// GLOBAL async channels — a declared `async` parameter, an `async` field, an
// async closure literal. A PLAIN parameter that is async only at one call site
// was invisible to them, so an adapted instance could store it into a plain
// field (or return it through a plain declared return) and the value escaped
// with its asyncness stripped: a `|| i32` field holding a promise, and a later
// call through it typed `i32` and yielding `Promise { <pending> }`.
//
// The checks now also run per instance, with the instance's bits — the same
// two checks, the same refusal, given the context they were missing.

#[test]
fn an_adapted_instance_cannot_store_its_now_async_parameter_into_a_plain_field() {
    // The escape, minimal: `f` is plain (so it adapts), async at this one call
    // site, and stored into a field whose type awaits nothing. Before the fix
    // this compiled and `(holder.hook)()` returned a promise typed `i32`.
    assert_fails_with(
        r#"
        import std::time::sleep;
        struct Holder {
            hook: || i32,
        }
        fun install(f: || i32): Holder {
            Holder { hook = f }
        }
        fun main() {
            let holder = install(|| { sleep(1); 2 });
            let _ = (holder.hook)();
        }
        "#,
        "reaches `Holder`'s field `hook`, which awaits nothing",
    );
}

#[test]
fn an_adapted_instance_cannot_assign_its_now_async_parameter_into_a_plain_field() {
    // The assignment half of the same store position — the global check already
    // covered both shapes, and so does its per-instance twin.
    assert_fails_with(
        r#"
        import std::time::sleep;
        struct Holder {
            hook: || i32,
        }
        fun install(f: || i32): Holder {
            mut holder = Holder { hook = || 0 };
            holder.hook = f;
            holder
        }
        fun main() {
            let _ = install(|| { sleep(1); 4 });
        }
        "#,
        "reaches `Holder`'s field `hook`, which awaits nothing",
    );
}

#[test]
fn an_adapted_instance_cannot_return_its_now_async_parameter_through_a_plain_return() {
    // The sibling escape position. Handing the parameter straight back is not
    // A.4's effect-row case — the value's asyncness IS the parameter's, no
    // effect variable connects two positions — so it is refused for the same
    // reason the field store is.
    assert_fails_with(
        r#"
        import std::time::sleep;
        fun pass(f: || i32): || i32 {
            f
        }
        fun main() {
            let got = pass(|| { sleep(1); 3 });
            let _ = got();
        }
        "#,
        "reaches `pass`'s declared return type, which awaits nothing",
    );
}

#[test]
fn compose_with_an_async_argument_is_the_error_a4_said_it_was() {
    // A.4: "`fun compose(f, g): |A| C { |a| g(f(a)) }` with an async `f` stays
    // an error at the return". It did not — the returned closure is async only
    // through the instance's bits, which the global check could not see, so
    // this compiled and printed a promise. It is now the error A.4 described.
    assert_fails_with(
        r#"
        import std::time::sleep;
        fun compose(f: |i32| i32, g: |i32| i32): |i32| i32 {
            |a| g(f(a))
        }
        fun main() {
            let h = compose(|x| { sleep(1); x + 1 }, |y| y * 2);
            let _ = h(3);
        }
        "#,
        "reaches `compose`'s declared return type, which awaits nothing",
    );
}

#[test]
fn an_adapted_instance_storing_into_a_void_field_stays_spawn() {
    // A.3: void positions keep spawn semantics at every boundary — end to end,
    // through the store. The per-instance check cannot reach this even in
    // principle: a void-returning parameter is not adaptive (`adaptive_params_of`
    // requires a value return), so it never carries a bit and nothing can escape
    // through it. Pinned anyway, because that is the behaviour A.3 promises and
    // the upstream gate is not where a reader would look for it.
    assert_compiles(
        r#"
        import std::time::sleep;
        struct Holder {
            on_done: || void,
        }
        fun install(f: || void): Holder {
            Holder { on_done = f }
        }
        fun main() {
            let _ = install(|| { sleep(1); });
        }
        "#,
    );
}

#[test]
fn an_adapted_instance_storing_into_an_async_field_is_the_fix() {
    // The steer the diagnostic gives, proven to work: declaring the field
    // `async || T` makes the store legal and awaits the later call through it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        struct Holder {
            hook: async || i32,
        }
        fun install(f: || i32): Holder {
            Holder { hook = f }
        }
        fun main() {
            let holder = install(|| { sleep(1); 2 });
            print((holder.hook)());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_sync_instance_storing_the_same_parameter_into_a_plain_field_stays_legal() {
    // The instance control: the SAME function, same store, a synchronous
    // argument. Nothing escapes, nothing is refused — the check is per
    // instance, not per function.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder {
            hook: || i32,
        }
        fun install(f: || i32): Holder {
            Holder { hook = f }
        }
        fun main() {
            let holder = install(|| 7);
            print((holder.hook)());
        }
        "#,
        "7\n",
    );
}

#[test]
fn transitive_adaptation_still_rides_past_a_store_free_body() {
    // The guard on the other side: A.4 keeps transitive adaptation LEGAL (a
    // call-position flow). Refusing escapes must not disturb it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::time::sleep;
        fun helper(f: || i32): i32 {
            f() + 1
        }
        fun main() {
            print(helper(|| { sleep(1); 10 }));
        }
        "#,
        "11\n",
    );
}
