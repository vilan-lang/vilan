//! The std surface (I4), bounded-generic dispatch (B55/B56/B58), resource
//! payloads through generics (B62/B65/B66/B101), and method resolution (B57,
//! B72, B74, B98).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- I4: the std surface batch (proposal/std-surface.md) ---------------------
//
// The ranked gap list's v1 cut: `List`'s search (`find`/`contains`/`index_of`),
// order (`reverse`/`sort`/`sort_by`), splice (`insert`/`remove`) and `join`
// methods, plus `f64`/`f32.clamp`. Placement is part of the contract — every
// method below except `join` must resolve from a program that imports nothing
// but `print` (`the_std_surface_batch_needs_no_import` pins that), and `join`'s
// `Display` bound forces it into `display.vl`, which the steering diagnostic
// further down covers.

#[test]
fn list_find_returns_the_first_match() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let xs = [3, 8, 5, 9];
            print(xs.find(|n| n > 4).unwrap_or(0));   // 8 — first, not last
            print(xs.find(|n| n > 90).is_none());     // true
            mut empty: List<i32> = [];
            print(empty.find(|n| n > 0).is_none());   // true
        }
        "#,
        "8\ntrue\ntrue\n",
    );
}

#[test]
fn list_find_short_circuits_at_the_first_match() {
    // The hand-rolled shape it replaces (proposal/std-surface.md §2.2) had no
    // short-circuit; this one does, and the visit count proves it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let xs = [1, 2, 3, 4];
            mut visits = 0;
            let found = xs.find(|n| { visits += 1; n > 1 });
            print(found.unwrap_or(0));  // 2
            print(visits);              // 2 — stopped, did not walk 3 and 4
        }
        "#,
        "2\n2\n",
    );
}

#[test]
fn list_contains_and_index_of_compare_by_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let xs = [10, 20, 30, 20];
            print(xs.contains(20));                 // true
            print(xs.contains(25));                 // false
            print(xs.index_of(20).unwrap_or(-1));   // 1 — the first
            print(xs.index_of(30).unwrap_or(-1));   // 2
            print(xs.index_of(99).is_none());       // true
            let words = ["a", "b"];
            print(words.contains("b"));             // true
        }
        "#,
        "true\nfalse\n1\n2\ntrue\ntrue\n",
    );
}

#[test]
fn list_reverse_returns_a_new_list() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let xs = [1, 2, 3];
            let ys = xs.reverse();
            print(ys[0]);       // 3
            print(ys[2]);       // 1
            print(xs[0]);       // 1 — the receiver is untouched
            mut empty: List<i32> = [];
            print(empty.reverse().len());  // 0
        }
        "#,
        "3\n1\n1\n0\n",
    );
}

#[test]
fn list_sort_orders_by_ord() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let xs = [3, 1, 2];
            let sorted = xs.sort();
            print(sorted[0]);
            print(sorted[1]);
            print(sorted[2]);
            print(xs[0]);       // 3 — the receiver is untouched
            let words = ["pear", "apple", "fig"];
            print(words.sort()[0]);   // apple — `str` is `Ord`
        }
        "#,
        "1\n2\n3\n3\napple\n",
    );
}

#[test]
fn list_sort_is_not_a_lexicographic_string_sort() {
    // The native `Array.prototype.sort` defaults to comparing STRINGIFIED
    // elements, which would order these 1, 10, 2. `sort` passes `Ord::compare`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let sorted = [10, 2, 1].sort();
            print(sorted[0]);
            print(sorted[1]);
            print(sorted[2]);
        }
        "#,
        "1\n2\n10\n",
    );
}

#[test]
fn list_sort_by_uses_the_comparator() {
    assert_compiles_and_runs(
        r#"
        import std::{ print, compare::Ordering };
        fun main() {
            let xs = [3, 1, 2];
            let descending = xs.sort_by(|a, b| {
                if a > b { Ordering::Less } else {
                    if a < b { Ordering::Greater } else { Ordering::Equal }
                }
            });
            print(descending[0]);
            print(descending[1]);
            print(descending[2]);
            print(xs[0]);   // 3 — the receiver is untouched
        }
        "#,
        "3\n2\n1\n3\n",
    );
}

#[test]
fn list_sort_by_is_stable() {
    // proposal/std-surface.md §3.1: stability is a hard requirement, inherited
    // from ECMA-262's stable `Array.prototype.sort` (ES2019). Two elements per
    // key, distinct secondary data — equal keys must keep their input order.
    assert_compiles_and_runs(
        r#"
        import std::{ print, compare::Ordering };
        struct Item {
            key: i32,
            tag: str,
        }
        fun main() {
            mut xs: List<Item> = [];
            xs.push(Item { key = 1, tag = "a" });
            xs.push(Item { key = 0, tag = "b" });
            xs.push(Item { key = 1, tag = "c" });
            xs.push(Item { key = 0, tag = "d" });
            mut out = "";
            for item in xs.sort_by(|a, b| a.key.compare(b.key)) {
                out = out + item.tag;
            }
            print(out);   // bdac — not badc, not bdca
        }
        "#,
        "bdac\n",
    );
}

#[test]
fn list_insert_shifts_the_tail_and_appends_at_len() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [1, 2, 4];
            xs.insert(2, 3);
            print(xs.len());   // 4
            print(xs[1]);      // 2
            print(xs[2]);      // 3
            print(xs[3]);      // 4
            xs.insert(4, 5);   // index == len is legal: an append
            print(xs[4]);      // 5
            xs.insert(0, 0);
            print(xs[0]);      // 0
            print(xs.len());   // 6
        }
        "#,
        "4\n2\n3\n4\n5\n0\n6\n",
    );
}

#[test]
fn list_remove_returns_the_element_and_closes_the_gap() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [1, 2, 3];
            print(xs.remove(1));   // 2
            print(xs.len());       // 2
            print(xs[0]);          // 1
            print(xs[1]);          // 3
            print(xs.remove(1));   // 3 — the last element
            print(xs.len());       // 1
        }
        "#,
        "2\n2\n1\n3\n3\n1\n",
    );
}

#[test]
fn list_remove_out_of_bounds_panics_like_the_subscript() {
    // proposal/std-surface.md §3.2: `remove`/`insert` follow `[]`'s panic, not
    // `get`'s `Option` — a bad INDEX is a caller bug, and the wording is `[]`'s.
    assert_run_panics(
        r#"
        fun main() {
            mut xs = [1, 2, 3];
            xs.remove(5);
        }
        main();
        "#,
        "index out of bounds: the length is 3 but the index is 5",
    );
}

#[test]
fn list_remove_at_a_negative_index_panics() {
    assert_run_panics(
        r#"
        fun main() {
            mut xs = [1, 2, 3];
            xs.remove(-1);
        }
        main();
        "#,
        "index out of bounds: the length is 3 but the index is -1",
    );
}

#[test]
fn list_insert_past_len_panics() {
    // `index == len` appends; `index > len` is the caller bug.
    assert_run_panics(
        r#"
        fun main() {
            mut xs = [1, 2, 3];
            xs.insert(4, 9);
        }
        main();
        "#,
        "index out of bounds: the length is 3 but the index is 4",
    );
}

#[test]
fn list_join_renders_display_elements() {
    assert_compiles_and_runs(
        r#"
        import std::{ print, display::Display };
        fun main() {
            let words = ["alpha", "beta", "gamma"];
            print(words.join(", "));
            print([1, 2, 3].join("-"));
            mut empty: List<str> = [];
            print(empty.join(", ") == "");   // true
            print(["solo"].join(", "));
        }
        "#,
        "alpha, beta, gamma\n1-2-3\ntrue\nsolo\n",
    );
}

#[test]
fn list_join_renders_a_user_display_impl() {
    assert_compiles_and_runs(
        r#"
        import std::{ print, display::Display };
        struct Tag {
            name: str,
        }
        impl Tag with Display {
            fun to_string(self): str {
                i"<{self.name}>"
            }
        }
        fun main() {
            mut tags: List<Tag> = [];
            tags.push(Tag { name = "red" });
            tags.push(Tag { name = "blue" });
            print(tags.join(" "));
        }
        "#,
        "<red> <blue>\n",
    );
}

#[test]
fn floats_clamp_without_being_ord() {
    // proposal/std-surface.md §1.4: `clamp` was `Ord`'s trait default, and the
    // floats are deliberately not `Ord` (NaN), so they had none. Same recipe.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            print(5f.clamp(0f, 3f));      // 3
            print((0f - 2f).clamp(0f, 3f)); // 0
            print(1.5f.clamp(0f, 3f));    // 1.5
            let wide = 5f.as_f32();
            print(wide.clamp(0f.as_f32(), 3f.as_f32()));  // 3
        }
        "#,
        "3\n0\n1.5\n3\n",
    );
}

#[test]
fn the_std_surface_batch_needs_no_import() {
    // The placement contract (proposal/std-surface.md §1.1): a method's
    // discoverability is decided by which std file it lives in. Everything in
    // the batch except `join` sits in an always-loaded module, so a program
    // that imports nothing but `print` reaches all of it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [3, 1, 2];
            print(xs.reverse()[0]);
            print(xs.sort()[0]);
            print(xs.sort_by(|a, b| a.compare(b))[2]);
            print(xs.contains(2));
            print(xs.index_of(2).unwrap_or(-1));
            print(xs.find(|n| n > 2).unwrap_or(0));
            xs.insert(0, 9);
            print(xs.remove(0));
            print(5f.clamp(0f, 3f));
        }
        "#,
        "2\n1\n3\ntrue\n2\n3\n9\n3\n",
    );
}

// --- I4's open tail: Map/Set parity (proposal/std-surface.md §1.2/§3) --------
//
// The unranked "Map/Set parity" row v1 left unshipped: `entries`/
// `contains_value` on `Map`, `union`/`intersection`/`difference` on `Set`.
// `map`/`filter`/`for_each` on either are deliberately NOT here — the audit
// never settled what they would return (a `Map`, a `List`, values only, pairs?)
// and, once `entries()` exists, the composable route
// (`map.entries().iter()...`) already covers the need with no ambiguity to
// invent. `str` carries no ranked or unranked gap in the audit (§1.3) — nothing
// to pin.

#[test]
fn map_entries_pairs_keys_and_values_in_insertion_order() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        fun main() {
            mut scores: Map<str, i32> = Map::new();
            scores.insert("alice", 1);
            scores.insert("bob", 2);
            scores.insert("alice", 99);   // overwrite -- position does not move
            mut order = "";
            mut total = 0;
            for entry in scores.entries() {
                order = order + entry.0;
                total = total + entry.1;
            }
            print(order);                 // alicebob -- alice keeps its first slot
            print(total);                 // 101 -- the overwritten value, not 1 + 2
            print(scores.entries().len()); // 2 -- overwrite is not a new pair
        }
        "#,
        "alicebob\n101\n2\n",
    );
}

#[test]
fn map_entries_on_an_empty_map_is_empty() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        fun main() {
            mut empty: Map<str, i32> = Map::new();
            print(empty.entries().len());   // 0
            print(empty.entries().is_empty()); // true
        }
        "#,
        "0\ntrue\n",
    );
}

#[test]
fn map_contains_value_compares_by_value_not_by_key() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        fun main() {
            mut scores: Map<str, i32> = Map::new();
            scores.insert("x", 5);
            scores.insert("y", 5);   // a duplicate value under a different key
            print(scores.contains_value(5));    // true
            print(scores.contains_value(6));    // false -- absent
            print(scores.contains_key("z"));    // false -- "z" was never a key
        }
        "#,
        "true\nfalse\nfalse\n",
    );
}

#[test]
fn map_contains_value_on_an_empty_map_is_false() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        fun main() {
            mut empty: Map<str, i32> = Map::new();
            print(empty.contains_value(0));
        }
        "#,
        "false\n",
    );
}

#[test]
fn set_union_combines_and_dedupes_the_overlap() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        fun main() {
            mut a: Set<i32> = Set::new();
            a.insert(1);
            a.insert(2);
            a.insert(3);
            mut b: Set<i32> = Set::new();
            b.insert(2);
            b.insert(3);
            b.insert(4);
            let combined = a.union(b);
            print(combined.len());          // 4 -- {1,2,3,4}, 2 and 3 not doubled
            print(combined.contains(1));    // true
            print(combined.contains(4));    // true
            print(a.len());                 // 3 -- the receiver is untouched
        }
        "#,
        "4\ntrue\ntrue\n3\n",
    );
}

#[test]
fn set_union_with_an_empty_set_is_identity_either_direction() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        fun main() {
            mut a: Set<i32> = Set::new();
            a.insert(1);
            a.insert(2);
            mut empty: Set<i32> = Set::new();
            print(a.union(empty).len());       // 2
            print(empty.union(a).len());       // 2
            print(empty.union(empty).len());   // 0 -- both sides empty
        }
        "#,
        "2\n2\n0\n",
    );
}

#[test]
fn set_intersection_keeps_only_the_shared_elements() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        fun main() {
            mut a: Set<i32> = Set::new();
            a.insert(1);
            a.insert(2);
            a.insert(3);
            mut b: Set<i32> = Set::new();
            b.insert(2);
            b.insert(3);
            b.insert(4);
            let shared = a.intersection(b);
            print(shared.len());            // 2
            print(shared.contains(2));      // true
            print(shared.contains(1));      // false

            mut disjoint: Set<i32> = Set::new();
            disjoint.insert(100);
            print(a.intersection(disjoint).len());   // 0 -- no overlap
        }
        "#,
        "2\ntrue\nfalse\n0\n",
    );
}

#[test]
fn set_difference_keeps_elements_absent_from_the_other_side() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        fun main() {
            mut a: Set<i32> = Set::new();
            a.insert(1);
            a.insert(2);
            a.insert(3);
            mut b: Set<i32> = Set::new();
            b.insert(2);
            b.insert(3);
            let remainder = a.difference(b);
            print(remainder.len());          // 1
            print(remainder.contains(1));    // true
            print(remainder.contains(2));    // false

            print(a.difference(a).len());    // 0 -- a set minus itself is empty
            mut empty: Set<i32> = Set::new();
            print(a.difference(empty).len()); // 3 -- nothing removed
        }
        "#,
        "1\ntrue\nfalse\n0\n3\n",
    );
}

#[test]
fn the_map_set_parity_batch_needs_only_its_own_type_import() {
    // Placement pin, mirroring `the_std_surface_batch_needs_no_import`: `map.vl`
    // pulls `compare::PartialEq` in transitively, so `contains_value` needs no
    // separate import beyond `Map` itself; `union`/`intersection`/`difference`
    // carry no extra bound beyond `Set`'s own `T: Hashable`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::set::Set;
        fun main() {
            mut scores: Map<str, i32> = Map::new();
            scores.insert("a", 1);
            mut total = 0;
            for entry in scores.entries() {
                total = total + entry.1;
            }
            print(total);
            print(scores.contains_value(1));

            mut xs: Set<i32> = Set::new();
            xs.insert(1);
            mut ys: Set<i32> = Set::new();
            ys.insert(2);
            print(xs.union(ys).len());
            print(xs.intersection(ys).len());
            print(xs.difference(ys).len());
        }
        "#,
        "1\ntrue\n2\n0\n1\n",
    );
}

// --- B85: `for x in <set>` fires for every form of the iterable ---------------
//
// The `Set` loop lowering (`__set_iter`, walking the backing map's values) used
// to be chosen from a type lookup on the ITERABLE EXPRESSION, and that lookup is
// silent for every expression that stores no type on its own id — a parameter
// (`self` above all), a call result, a `*view`. Those loops fell through to a
// bare `for...of` over the struct's one-element field array, so a 3-element set
// counted 1, silently. The type now comes from the analyzer's own per-loop
// record (`for_each_iterable_types`), which is total by construction; these pins
// cover one shape of iterable each, plus the sibling containers whose loops must
// keep lowering exactly as before.

#[test]
fn a_set_loop_over_self_inside_its_own_generic_impl_walks_the_elements() {
    // The recorded repro (std-surface.md §7.7). `self` is the case std's own
    // `Set` methods have been routing around via `self.table.values()` by
    // convention since I4; the direct form is what a user writes first.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        impl Set<type T: Hashable> {
            fun probe(self): i32 {
                mut n = 0;
                for x in self {
                    n = n + 1;
                }
                n
            }
        }
        fun main() {
            mut s: Set<i32> = Set::new();
            s.insert(1);
            s.insert(2);
            s.insert(3);
            print(s.probe());   // 3, not 1
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_set_loop_over_self_yields_the_elements_not_the_backing_field() {
    // Counting alone would also pass a loop that walked the right NUMBER of
    // wrong things, so this one sums the elements: the backing-array lowering
    // yields the `NativeMap` itself, which does not add.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        impl Set<type T: Hashable> {
            fun total(self): i32 {
                mut sum = 0;
                for x in self {
                    sum = sum + 1;
                }
                sum
            }
        }
        impl Set<i32> {
            fun sum(self): i32 {
                mut sum = 0;
                for x in self {
                    sum = sum + x;
                }
                sum
            }
        }
        fun main() {
            mut s: Set<i32> = Set::new();
            s.insert(10);
            s.insert(20);
            s.insert(30);
            print(s.total());   // 3
            print(s.sum());     // 60 -- real elements, in insertion order
        }
        "#,
        "3\n60\n",
    );
}

#[test]
fn a_set_loop_inside_its_own_impl_builds_a_correct_union() {
    // The payoff, and the shape that found the bug: a `union` written the direct
    // way (`for value in self` / `for value in other`) rather than through
    // `self.table.values()`. The first draft of std's `union` was exactly this
    // and returned length 1 for a 4-element union. std's existing methods keep
    // their `.table.values()` idiom (rewriting them would be churn) -- what this
    // pins is that the convention is no longer load-bearing.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        impl Set<type T: Hashable> {
            fun merged(self, other: Set<T>): Set<T> {
                mut result: Set<T> = Set::new();
                for value in self {
                    result.insert(value);
                }
                for value in other {
                    result.insert(value);
                }
                result
            }
        }
        fun main() {
            mut a: Set<i32> = Set::new();
            a.insert(1);
            a.insert(2);
            a.insert(3);
            mut b: Set<i32> = Set::new();
            b.insert(3);
            b.insert(4);
            print(a.merged(b).len());   // 4, not 1
        }
        "#,
        "4\n",
    );
}

#[test]
fn a_set_loop_over_a_mut_self_receiver_walks_the_elements() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        impl Set<type T: Hashable> {
            fun probe(&mut self): i32 {
                mut n = 0;
                for x in self {
                    n = n + 1;
                }
                n
            }
        }
        fun main() {
            mut s: Set<i32> = Set::new();
            s.insert(1);
            s.insert(2);
            print(s.probe());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_set_loop_over_a_plain_parameter_walks_the_elements() {
    // Not an `impl` at all, and not generic: `self` was only the most common
    // parameter, never the special one. A concrete `Set<i32>` parameter and a
    // generic `Set<T>` one were equally broken.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        fun count_concrete(s: Set<i32>): i32 {
            mut n = 0;
            for x in s {
                n = n + 1;
            }
            n
        }
        fun count_generic<T: Hashable>(s: Set<T>): i32 {
            mut n = 0;
            for x in s {
                n = n + 1;
            }
            n
        }
        fun main() {
            mut s: Set<i32> = Set::new();
            s.insert(1);
            s.insert(2);
            s.insert(3);
            print(count_concrete(s));
            print(count_generic(s));
        }
        "#,
        "3\n3\n",
    );
}

#[test]
fn a_set_loop_over_a_call_result_or_a_view_walks_the_elements() {
    // The other two forms that store no type on their own expr id. A `let`
    // binding and a field access always worked (both are recorded), and are
    // here as the regression half of the same pin.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        struct Holder {
            inner: Set<i32>,
        }
        fun make(): Set<i32> {
            mut s: Set<i32> = Set::new();
            s.insert(1);
            s.insert(2);
            s.insert(3);
            s
        }
        fun from_call(): i32 {
            mut n = 0;
            for x in make() {
                n = n + 1;
            }
            n
        }
        fun from_view(s: &Set<i32>): i32 {
            mut n = 0;
            for x in *s {
                n = n + 1;
            }
            n
        }
        fun from_field(holder: Holder): i32 {
            mut n = 0;
            for x in holder.inner {
                n = n + 1;
            }
            n
        }
        fun main() {
            let s = make();
            print(from_call());                       // call result
            print(from_view(&s));                     // *view
            print(from_field(Holder { inner = s }));  // field access
            mut n = 0;
            for x in s {                              // plain `let` binding
                n = n + 1;
            }
            print(n);
        }
        "#,
        "3\n3\n3\n3\n",
    );
}

#[test]
fn a_set_loop_survives_nesting_and_a_closure_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        fun make(): Set<i32> {
            mut s: Set<i32> = Set::new();
            s.insert(1);
            s.insert(2);
            s.insert(3);
            s
        }
        fun main() {
            mut n = 0;
            for s in [make(), make()] {
                for x in s {
                    n = n + 1;
                }
            }
            print(n);   // 6 -- the loop binding is a `Set`, not an element
            let count = |s: Set<i32>| {
                mut c = 0;
                for x in s {
                    c = c + 1;
                }
                c
            };
            print(count(make()));
        }
        "#,
        "6\n3\n",
    );
}

#[test]
fn the_sibling_containers_iterate_inside_their_own_impls_too() {
    // The sweep's other half. `List` and `str` are JS-native iterables, so their
    // loops never needed a type-driven lowering and were never broken -- pinned
    // here so the B85 change is proven not to have moved them either.
    assert_compiles_and_runs(
        r#"
        import std::print;
        impl List<type T> {
            fun count(self): i32 {
                mut n = 0;
                for x in self {
                    n = n + 1;
                }
                n
            }
        }
        impl str {
            fun letters(self): i32 {
                mut n = 0;
                for c in self {
                    n = n + 1;
                }
                n
            }
        }
        fun main() {
            print([1, 2, 3].count());
            print("abc".letters());
        }
        "#,
        "3\n3\n",
    );
}

#[test]
// Found by B85's sweep as a known bug; closed at the same cut by B80's
// for-loop rule — the two lanes converged on it from opposite sides.
fn a_for_loop_over_a_map_is_refused_rather_than_walking_the_backing_field() {
    // `Map` has NO native loop lowering and no `next` -- it is not iterable at
    // all. But the analyzer only refuses an uniterable subject when it is a
    // generic or a bare trait `Self` (B56); a STRUCT with no protocol method
    // falls through to a native `for...of`, which over `Map`'s flat field array
    // yields its one backing `NativeMap`. So `for entry in scores` compiles and
    // "iterates" exactly once, whatever the map holds -- at every call site, not
    // just inside `Map`'s own impl, so this is B85's neighbour and not B85.
    //
    // The fix is B56's own rule ("this is an error, not a silent lowering")
    // extended from generics to structs, with the natively-iterable built-ins
    // (`List`, `Set`, `str`, `[T; n]`) as the exception. Giving `Map` a loop
    // lowering instead would be new surface: std-surface.md §7.7 declined to
    // settle what `Map` iteration yields (keys? values? pairs?), and
    // `map.entries()` already covers the need.
    assert_fails_with(
        r#"
        import std::print;
        import std::map::Map;
        fun main() {
            mut scores: Map<str, i32> = Map::new();
            scores.insert("a", 1);
            scores.insert("b", 2);
            mut n = 0;
            for entry in scores {
                n = n + 1;
            }
            print(n);
        }
        "#,
        "cannot iterate",
    );
}

// --- I4: the `to_string()` steering diagnostic (proposal/std-surface.md §5) ---
//
// `display.vl` sits outside the always-loaded core set and outside its
// transitive import closure, so `42.to_string()` fails with a bare "no method"
// until the program names `Display`. The steer is a fourth appended hint at the
// `MethodLookup::NoMethod` arm, backed by a std-wide index of trait-provided
// method names built in the same lazy pass as the B4 import steer's.

#[test]
fn a_missing_to_string_steers_to_the_display_import() {
    assert_fails_spanning(
        r#"
        import std::print;
        fun main() {
            let x = 42;
            print(x.to_string());
        }
        "#,
        "to_string",
        "i32 has no method 'to_string'; import std::display::Display to use it \
         (`import std::display::Display;`)",
    );
}

#[test]
fn the_to_string_steer_covers_every_display_impl_subject() {
    for (literal, type_name) in [
        ("42", "i32"),
        ("true", "bool"),
        ("1.5f", "f64"),
        ("7n", "BigInt"),
    ] {
        let source = format!(
            r#"
            fun main() {{
                let value = {literal};
                let _ = value.to_string();
            }}
            "#
        );
        let diagnostics = failure_diagnostics(&source);
        assert!(
            diagnostics.iter().any(|(message, _)| message
                == &format!(
                    "{type_name} has no method 'to_string'; import std::display::Display \
                     to use it (`import std::display::Display;`)"
                )),
            "no steered diagnostic for {type_name}; got: {diagnostics:#?}"
        );
    }
}

#[test]
fn the_join_miss_steers_to_the_display_import() {
    // `join`'s `Display` bound forces it into `display.vl`, so a plain `List`
    // program cannot see it — the steer is the mitigation the audit specified.
    assert_fails_spanning(
        r#"
        import std::print;
        fun main() {
            let words = ["a", "b"];
            print(words.join(", "));
        }
        "#,
        "join",
        "import std::display::Display to use it",
    );
}

#[test]
fn the_import_steer_does_not_survive_the_import() {
    // B5's no-repetition spirit: once `Display` is imported the call resolves,
    // so there is no diagnostic left to carry a hint.
    assert_compiles_and_runs(
        r#"
        import std::{ print, display::Display };
        fun main() {
            print(42.to_string());
        }
        "#,
        "42\n",
    );
}

#[test]
fn a_method_no_std_trait_provides_carries_no_import_steer() {
    let diagnostics = failure_diagnostics(
        r#"
        fun main() {
            let x = 42;
            let _ = x.frobnicate();
        }
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message == "i32 has no method 'frobnicate'"),
        "expected the bare no-method error; got: {diagnostics:#?}"
    );
}

#[test]
fn the_import_steer_does_not_fire_for_a_user_type() {
    // The index is keyed by the SUBJECT HEAD, so a user type that happens to
    // miss a method std provides on some other type must stay unsteered.
    let diagnostics = failure_diagnostics(
        r#"
        struct Point {
            x: i32,
        }
        fun main() {
            let p = Point { x = 1 };
            let _ = p.to_string();
        }
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message == "Point has no method 'to_string'"),
        "expected the bare no-method error; got: {diagnostics:#?}"
    );
}

#[test]
fn an_unsatisfied_bound_is_reported_as_a_bound_not_as_a_steered_miss() {
    // Why the steer needs no "is that module already loaded?" guard: once
    // `display` loads, `join` RESOLVES, so a `T` without `Display` fails on the
    // BOUND, at the bound's own site — the "no method" arm is never reached and
    // there is no diagnostic left to tell the reader to import what they have.
    let diagnostics = failure_diagnostics(
        r#"
        import std::{ print, display::Display };
        struct Opaque {
            n: i32,
        }
        fun main() {
            mut xs: List<Opaque> = [];
            xs.push(Opaque { n = 1 });
            print(xs.join(", "));
        }
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("'Opaque' does not implement trait 'Display'")),
        "expected the bound failure; got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(message, _)| message.contains("has no method 'join'")),
        "a loaded module's bounded impl must not read as a missing method; got: {diagnostics:#?}"
    );
}

// --- B55/B56: bounded-generic dispatch through a re-dispatched callee, and
// --- `for` over a non-concrete subject (proposal/iterator-adapters.md P2/P3) --

/// The adapter shape: `self.upstream.next()` where `upstream: U, U: Iter<T>`,
/// driven by the `for`-loop protocol. The loop emitted its `next` callee by
/// BARE ID — the concrete-function path — so the generic `Passthrough::next`
/// body was walked with no substitution, `U` never bound, and the inner
/// bounded call resolved to the trait's abstract member: an EMPTY function
/// body, exit 0, `TypeError` at runtime.
#[test]
fn a_for_loop_over_a_generic_adapter_drives_its_upstream() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> { fun next(&mut self): Option<T>; }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        struct Passthrough<U, T> { upstream: U }
        impl Passthrough<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> { self.upstream.next() }
        }

        fun main() {
            mut p = Passthrough { upstream = Counting { at = 0, limit = 3 } };
            for v in p { print(v); }
        }
        "#,
        "1\n2\n3\n",
    );
}

/// The same shape reached through a DIRECT call rather than the loop — the
/// control that was already correct, pinned so the loop fix does not regress
/// it.
#[test]
fn a_direct_call_on_a_generic_adapter_drives_its_upstream() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> { fun next(&mut self): Option<T>; }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        struct Passthrough<U, T> { upstream: U }
        impl Passthrough<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> { self.upstream.next() }
        }

        fun main() {
            mut p = Passthrough { upstream = Counting { at = 0, limit = 3 } };
            match p.next() { Some(let v) => print(v), None => print(-1) }
        }
        "#,
        "1\n",
    );
}

/// B55's second trigger: an adapter CONSTRUCTED by a trait default. `Self`
/// nested in the default's return type (`Taken<Self, T>`) was left as the
/// abstract `Type::Trait`, so `Taken`'s `U` bound to the bare trait and
/// `self.upstream.next()` resolved to the trait's abstract member.
#[test]
fn a_trait_default_constructing_an_adapter_binds_self_to_the_receiver() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        struct Taken<U, T> { upstream: U, remaining: i32 }

        trait Iter<T> {
            fun next(&mut self): Option<T>;
            fun taken(self, count: i32): Taken<Self, T> {
                Taken { upstream = self, remaining = count }
            }
        }

        impl Taken<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> {
                if self.remaining <= 0 { ret None; }
                self.remaining = self.remaining - 1;
                self.upstream.next()
            }
        }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        fun main() {
            mut t = Counting { at = 0, limit = 5 }.taken(3);
            match t.next() { Some(let v) => print(v), None => print(-1) }
        }
        "#,
        "1\n",
    );
}

/// The same `Self`-in-a-type-argument gap at its smallest: a trait default
/// returning `Wrap<Self>` must yield `Wrap<Dog>`, so the payload is callable.
/// It used to yield `Wrap<Marker>` — a bare trait type, which has no methods.
#[test]
fn a_trait_default_returning_a_wrapper_of_self_yields_the_concrete_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Wrap<U> { inner: U }

        trait Marker {
            fun tag(self): i32;
            fun wrapped(self): Wrap<Self> { Wrap { inner = self } }
        }

        struct Dog { legs: i32 }
        impl Dog with Marker { fun tag(self): i32 { self.legs } }

        fun main() { print(Dog { legs = 4 }.wrapped().inner.tag()); }
        "#,
        "4\n",
    );
}

/// A bare `Self` return keeps working — the exact-equality case the structural
/// substitution subsumes.
#[test]
fn a_trait_default_returning_bare_self_yields_the_concrete_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Marker {
            fun tag(self): i32;
            fun itself(self): Self { self }
        }

        struct Dog { legs: i32 }
        impl Dog with Marker { fun tag(self): i32 { self.legs } }

        fun main() { print(Dog { legs = 4 }.itself().tag()); }
        "#,
        "4\n",
    );
}

/// The full adapter pipeline both triggers compose into: a trait-default
/// constructor feeding a trait-default terminal over a bounded generic.
#[test]
fn a_trait_default_adapter_pipeline_runs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        import std::option::{Option, Some, None};

        struct Taken<U, T> { upstream: U, remaining: i32 }

        trait Iter<T> {
            fun next(&mut self): Option<T>;
            fun taken(self, count: i32): Taken<Self, T> {
                Taken { upstream = self, remaining = count }
            }
            fun to_list(mut self): List<T> {
                mut out = List::new();
                for {
                    match self.next() {
                        Some(let v) => out.push(v),
                        None => jump break,
                    }
                }
                out
            }
        }

        impl Taken<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> {
                if self.remaining <= 0 { ret None; }
                self.remaining = self.remaining - 1;
                self.upstream.next()
            }
        }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        fun main() { print(Counting { at = 0, limit = 5 }.taken(3).to_list().len()); }
        "#,
        "3\n",
    );
}

/// `Self` in a return type reaches a GENERIC receiver too: inside
/// `relay<T: Marker>`, `t.wrapped()` is `Wrap<T>` — abstract here, concrete at
/// each monomorphization. It used to stay `Wrap<Marker>`, and the payload read
/// as a bare trait value with no members.
#[test]
fn a_self_returning_default_called_on_a_generic_receiver_stays_generic() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Wrap<U> { inner: U }

        trait Marker {
            fun tag(self): i32;
            fun wrapped(self): Wrap<Self> { Wrap { inner = self } }
        }

        struct Dog { legs: i32 }
        impl Dog with Marker { fun tag(self): i32 { self.legs } }

        fun relay<T: Marker>(t: T): i32 { t.wrapped().inner.tag() }

        fun main() { print(relay(Dog { legs = 4 })); }
        "#,
        "4\n",
    );
}

/// Nesting: an adapter over an adapter, driven by the loop protocol. The
/// per-instantiation binding has to COMPOSE — the inner `Passthrough`'s `U` is
/// bound by the outer one's monomorphization.
#[test]
fn a_two_hop_generic_adapter_drives_through_both_layers() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> { fun next(&mut self): Option<T>; }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        struct Passthrough<U, T> { upstream: U }
        impl Passthrough<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> { self.upstream.next() }
        }

        fun main() {
            mut p = Passthrough {
                upstream = Passthrough { upstream = Counting { at = 0, limit = 3 } }
            };
            for v in p { print(v); }
        }
        "#,
        "1\n2\n3\n",
    );
}

/// B56, the trait-default shape: `self` inside a default is `Type::Trait`, so
/// the protocol guard missed and the loop fell through to a native `for...of`
/// over the struct's flat field array — a 3-element source yielded the two
/// FIELDS of `Counting`, silently.
#[test]
fn a_for_loop_over_self_in_a_trait_default_drives_the_protocol() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        import std::option::{Option, Some, None};

        trait Iter<T> {
            fun next(&mut self): Option<T>;
            fun to_list(mut self): List<T> {
                mut out = List::new();
                for v in self { out.push(v); }
                out
            }
        }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        fun main() { print(Counting { at = 0, limit = 3 }.to_list().len()); }
        "#,
        "3\n",
    );
}

/// B56, the bounded-generic shape: `it: I` with `I: Iter<i32>` is
/// `Type::Generic`, which missed the same guard. The loop summed the
/// receiver's FIELDS (`0 + 3`) instead of its elements (`1 + 2 + 3`).
#[test]
fn a_for_loop_over_a_bounded_generic_drives_the_protocol() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> { fun next(&mut self): Option<T>; }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        fun consume<I: Iter<i32>>(mut it: I): i32 {
            mut total = 0;
            for v in it { total = total + v; }
            total
        }

        fun main() { print(consume(Counting { at = 0, limit = 3 })); }
        "#,
        "6\n",
    );
}

/// B56, the diagnostic side: a `for` over a generic subject whose bounds
/// provide no iterator has nothing to iterate. It used to emit a native
/// `for...of` over an arbitrary value — `TypeError: it is not iterable` from a
/// clean compile. It must be a compile error.
#[test]
fn a_for_loop_over_an_unbounded_generic_is_a_compile_error() {
    assert_fails_with(
        r#"
        import std::io::print;

        fun consume<I>(it: I): i32 {
            mut total = 0;
            for _v in it { total = total + 1; }
            total
        }

        fun main() { print(consume(7)); }
        "#,
        "cannot iterate",
    );
}

/// B56, the generic-impl shape: `self` inside `impl Passthrough<type U: Iter<T>,
/// type T>` IS a `Type::Struct`, so it reached the protocol — and then hit
/// B55's bare-id emission. Both must hold at once.
#[test]
fn a_for_loop_over_self_in_a_generic_impl_drives_the_protocol() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> { fun next(&mut self): Option<T>; }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        struct Passthrough<U, T> { upstream: U }
        impl Passthrough<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> { self.upstream.next() }
            fun total(mut self): i32 {
                mut n = 0;
                for _v in self { n = n + 1; }
                n
            }
        }

        fun main() {
            print(Passthrough { upstream = Counting { at = 0, limit = 3 } }.total());
        }
        "#,
        "3\n",
    );
}

/// A generic bounded by a trait that provides no protocol method is the same
/// error as an unbounded one — the bound has to be an ITERATOR bound, not just
/// any bound.
#[test]
fn a_for_loop_over_a_generic_with_a_non_iterator_bound_is_a_compile_error() {
    assert_fails_with(
        r#"
        import std::io::print;

        trait Tag { fun tag(self): i32; }
        struct Counting { at: i32 }
        impl Counting with Tag { fun tag(self): i32 { 7 } }

        fun drive<I: Tag>(it: I) { for _v in it { print(1); } }
        fun main() { drive(Counting { at = 0 }); }
        "#,
        "cannot iterate",
    );
}

/// A trait default containing `for v in self` that no type ever instantiates
/// must still compile — the protocol resolution runs on the DECLARATION, and it
/// must not manufacture a diagnostic for a default nobody uses.
#[test]
fn an_uninstantiated_trait_default_iterating_self_still_compiles() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> {
            fun next(&mut self): Option<T>;
            fun count_them(mut self): i32 { mut n = 0; for _v in self { n = n + 1; } n }
        }

        struct Nothing { x: i32 }
        fun main() { print(Nothing { x = 1 }.x); }
        "#,
    );
}

/// Mixed: a bounded-generic FUNCTION whose `for` drives a generic ADAPTER —
/// B56's constraint dispatch feeding B55's per-instantiation emission.
#[test]
fn a_bounded_generic_function_drives_a_generic_adapter() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> { fun next(&mut self): Option<T>; }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        struct Passthrough<U, T> { upstream: U }
        impl Passthrough<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> { self.upstream.next() }
        }

        fun drive<I: Iter<i32>>(mut it: I) { for v in it { print(v); } }

        fun main() {
            drive(Passthrough { upstream = Counting { at = 0, limit = 3 } });
        }
        "#,
        "1\n2\n3\n",
    );
}

/// Mixed the other way: a TRAIT DEFAULT's `for v in self`, specialized for a
/// generic adapter — the `Self` re-dispatch and the adapter's own binding at
/// once.
#[test]
fn a_trait_default_loop_specialized_for_a_generic_adapter_counts_elements() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Iter<T> {
            fun next(&mut self): Option<T>;
            fun count_them(mut self): i32 { mut n = 0; for _v in self { n = n + 1; } n }
        }

        struct Counting { at: i32, limit: i32 }
        impl Counting with Iter<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
            }
        }

        struct Passthrough<U, T> { upstream: U }
        impl Passthrough<type U: Iter<T>, type T> with Iter<T> {
            fun next(&mut self): Option<T> { self.upstream.next() }
        }

        fun main() {
            print(Passthrough { upstream = Counting { at = 0, limit = 4 } }.count_them());
        }
        "#,
        "4\n",
    );
}

// --- B58: a bound on a trait's OWN generic parameter reaches its default ---
// --- bodies (proposal/iterator-adapters.md P4) -----------------------------

/// The headline shape: `trait Holder<T: Bound>`'s default calls one of
/// `Bound`'s members on a `T`-typed value. The analyzer always resolved the
/// member through the bound; codegen could not GROUND it — a trait default was
/// specialized under an EMPTY substitution, so the recorded
/// `GenericDispatch::OnConstraint(T, ..)` found no binding for `T` and fell
/// through to the trait's abstract member. (Pre-B55 that emitted an empty body
/// and threw at runtime; since B55's never-silent guard it is a hard error.)
#[test]
fn a_trait_default_calls_a_bound_member_on_its_own_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }

        trait Holder<T: Bound> {
            fun item(self): T;
            fun describe(self): str { self.item().label() }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> { fun item(self): Dog { Dog {} } }

        fun main() { print(DogBox {}.describe()); }
        "#,
        "dog\n",
    );
}

/// The dispatch must follow the INSTANTIATING type, not merely compile: two
/// impls of the same trait at different parameters, each producing its own
/// implementation's output from the one shared default body.
#[test]
fn a_trait_default_bound_call_dispatches_per_implementing_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }
        struct Cat {}
        impl Cat with Bound { fun label(self): str { "cat" } }

        trait Holder<T: Bound> {
            fun item(self): T;
            fun describe(self): str { self.item().label() }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> { fun item(self): Dog { Dog {} } }
        struct CatBox {}
        impl CatBox with Holder<Cat> { fun item(self): Cat { Cat {} } }

        fun main() { print(DogBox {}.describe()); print(CatBox {}.describe()); }
        "#,
        "dog\ncat\n",
    );
}

/// The guard rail, unchanged: an UNBOUNDED trait parameter still refuses
/// member access in a default body, with the same clean diagnostic it always
/// gave. The fix grounds a bound that was declared — it does not invent one.
#[test]
fn a_trait_default_cannot_call_a_member_on_an_unbounded_parameter() {
    assert_fails_with(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }

        trait Holder<T> {
            fun item(self): T;
            fun describe(self): str { self.item().label() }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> { fun item(self): Dog { Dog {} } }

        fun main() { print(DogBox {}.describe()); }
        "#,
        "cannot call method 'label' on T",
    );
}

/// A multi-bound parameter (`T: A + B`) reaches the members of BOTH bounds
/// from the same default body — the bound list is scanned, not just its head.
#[test]
fn a_trait_default_reaches_the_members_of_every_bound() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Speaks { fun speak(self): str; }
        trait Named { fun name(self): str; }

        struct Dog {}
        impl Dog with Speaks { fun speak(self): str { "woof" } }
        impl Dog with Named { fun name(self): str { "rex" } }

        trait Holder<T: Speaks + Named> {
            fun item(self): T;
            fun both(self): str { self.item().name() + " says " + self.item().speak() }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> { fun item(self): Dog { Dog {} } }

        fun main() { print(DogBox {}.both()); }
        "#,
        "rex says woof\n",
    );
}

/// The bound call nested inside a CLOSURE in the default body — the closure is
/// emitted within the specialized instance, so it inherits its substitution.
/// Two instantiations, so a stale binding cannot pass this.
#[test]
fn a_trait_default_bound_call_inside_a_closure_dispatches_per_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }
        struct Cat {}
        impl Cat with Bound { fun label(self): str { "cat" } }

        trait Holder<T: Bound> {
            fun item(self): T;
            fun shout(self): str { let render = || self.item().label(); render() }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> { fun item(self): Dog { Dog {} } }
        struct CatBox {}
        impl CatBox with Holder<Cat> { fun item(self): Cat { Cat {} } }

        fun main() { print(DogBox {}.shout()); print(CatBox {}.shout()); }
        "#,
        "dog\ncat\n",
    );
}

/// An impl that OVERRIDES the bound-using default is unaffected: the override
/// wins, including for a sibling default that calls it on `self`.
#[test]
fn an_impl_overriding_a_bound_using_default_still_wins() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }

        trait Holder<T: Bound> {
            fun item(self): T;
            fun describe(self): str { self.item().label() }
            fun twice(self): str { self.describe() + self.describe() }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> {
            fun item(self): Dog { Dog {} }
            fun describe(self): str { "OVERRIDE" }
        }

        fun main() { print(DogBox {}.twice()); }
        "#,
        "OVERRIDEOVERRIDE\n",
    );
}

/// Regression guard for B55's root cause B: `Self`-typed values in the SAME
/// body as parameter-typed ones. The default body seeds `T` now, and that must
/// not disturb the `Self` re-dispatch — `self.me()` still returns the concrete
/// receiver, and the round trip keeps both halves per instantiation.
#[test]
fn a_trait_default_mixes_self_typed_and_parameter_typed_values() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }
        struct Cat {}
        impl Cat with Bound { fun label(self): str { "cat" } }

        trait Holder<T: Bound> {
            fun item(self): T;
            fun tag(self): str;
            fun mixed(self): str { self.tag() + ":" + self.item().label() }
            fun me(self): Self { self }
            fun round(self): str { self.me().mixed() }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> {
            fun item(self): Dog { Dog {} }
            fun tag(self): str { "D" }
        }
        struct CatBox {}
        impl CatBox with Holder<Cat> {
            fun item(self): Cat { Cat {} }
            fun tag(self): str { "C" }
        }

        fun main() { print(DogBox {}.round()); print(CatBox {}.round()); }
        "#,
        "D:dog\nC:cat\n",
    );
}

/// The bound-to-bound leg — P4's own quoted repro. The default hands its `T`
/// to a binder that requires the SAME bound (`impl Wrap<type U: Bound>`).
/// This failed in the analyzer, not codegen: the return-type-only inference
/// re-bound the call's declared return `T` — which, `item` being a member of
/// the very trait whose default this is, IS the enclosing parameter — to the
/// expectation, dropping its bound and reporting "generic parameter 'U' is
/// missing the bound ': Bound'".
#[test]
fn a_trait_default_passes_its_parameter_to_a_binder_requiring_the_same_bound() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Wrap<U> { inner: U }
        impl Wrap<type U: Bound> { fun show(self): str { self.inner.label() } }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }

        trait Holder<T: Bound> {
            fun item(self): T;
            fun wrapped(self): Wrap<T> { Wrap { inner = self.item() } }
        }

        struct DogBox {}
        impl DogBox with Holder<Dog> { fun item(self): Dog { Dog {} } }

        fun main() { print(DogBox {}.wrapped().show()); }
        "#,
        "dog\n",
    );
}

/// A GENERIC impl: the trait argument is written in the impl's own terms
/// (`impl Bag<type E: Bound> with Holder<E>`), so grounding `T` takes two hops
/// — the impl's binder from the concrete receiver, then the trait's parameter
/// through it. Two element types, so the hop cannot be a constant.
#[test]
fn a_generic_impl_grounds_the_traits_parameter_through_its_own_binder() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Bound { fun label(self): str; }

        struct Dog {}
        impl Dog with Bound { fun label(self): str { "dog" } }
        struct Cat {}
        impl Cat with Bound { fun label(self): str { "cat" } }

        struct Bag<E> { element: E }
        trait Holder<T: Bound> {
            fun item(self): T;
            fun describe(self): str { self.item().label() }
        }
        impl Bag<type E: Bound> with Holder<E> { fun item(self): E { self.element } }

        fun main() {
            print(Bag { element = Dog {} }.describe());
            print(Bag { element = Cat {} }.describe());
        }
        "#,
        "dog\ncat\n",
    );
}

// --- B62: a pattern capture that takes ownership of a resource payload is
// destroyed at its scope end (`proposal/affine-moves.md` §7) ------------------

/// The `resource struct Res` + `Drop` preamble every B62 pin below shares.
const B62_PRELUDE: &str = r#"
    import std::print;
    import std::option::Option::{ self, Some, None };
    import std::drop::{ Drop, drop };
    resource struct Res {
        tag: str,
    }
    impl Res with Drop {
        fun drop(&mut self) {
            print(i"drop {self.tag}");
        }
    }
"#;

fn b62_program(body: &str) -> String {
    format!("{B62_PRELUDE}{body}")
}

#[test]
fn b62_a_match_leg_capture_is_destroyed_at_the_leg_end() {
    // The filed bug. Matching by value CONSUMES the subject (R6), so `o`'s own
    // scope-end teardown is suppressed and the capture is the payload's only
    // owner — but the drop planner never enrolled it, so the leg ended and
    // nothing was destroyed. Before the fix this printed `leg payload\nafter`
    // and leaked; it is the idiom B60 steers `if is_some { unwrap }` toward,
    // which is what made it urgent.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "payload" });
                match o {
                    Some(let r) => print(i"leg {r.tag}"),
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "leg payload\ndrop payload\nafter\n",
    );
}

#[test]
fn b62_a_nested_pattern_capture_is_destroyed() {
    // The capture sits two levels down (`Some((let r, let n))`) and only one
    // element is a resource: the enrollment is per CAPTURE, decided by the
    // capture's own type, not by the pattern's shape.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<(Res, i32)> = Some((Res { tag = "nested" }, 7));
                match o {
                    Some((let r, let n)) => print(i"leg {r.tag} {n}"),
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "leg nested 7\ndrop nested\nafter\n",
    );
}

#[test]
fn b62_a_mut_match_capture_is_destroyed() {
    // `mut` at a binder changes mutability, not ownership. (A `mut` resource
    // PARAMETER is rejected — it would copy — but a `mut` capture takes the
    // payload by move like any other, so it owns and drops.)
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "before" });
                match o {
                    Some(mut r) => {
                        r.tag = "mutated";
                        print(i"leg {r.tag}");
                    }
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "leg mutated\ndrop mutated\nafter\n",
    );
}

#[test]
fn b62_two_captures_in_one_leg_destroy_in_reverse_order() {
    // Drop timing and order (`docs/spec/memory.md`): still-owned resources drop
    // in REVERSE declaration order, and a leg's captures are declared left to
    // right, so the second payload dies first — the same order a scope's `let`s
    // get from `walk_scope_body`'s nested tries.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            resource enum Both {
                Pair(Res, Res),
                Nothing,
            }
            fun main() {
                let both = Both::Pair(Res { tag = "first" }, Res { tag = "second" });
                match both {
                    Both::Pair(let x, let y) => print(i"leg {x.tag} {y.tag}"),
                    Both::Nothing => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "leg first second\ndrop second\ndrop first\nafter\n",
    );
}

#[test]
fn b62_a_guarded_leg_capture_is_destroyed() {
    // A guarded leg binds nothing — B53 records its captures as ACCESSORS into
    // the subject and substitutes them at every reference — so the teardown
    // destroys through the accessor. The subject was consumed by the match, so
    // that slot has exactly one owner.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "guarded" });
                match o {
                    Some(let r) if r.tag == "guarded" => print(i"leg {r.tag}"),
                    Some(let other) => print(i"other {other.tag}"),
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "leg guarded\ndrop guarded\nafter\n",
    );
}

#[test]
fn b62_a_rejected_guard_destroys_nothing_and_the_next_leg_owns() {
    // B59's ordering decision, read for resources: a guard is a decision
    // procedure and a rejected leg must leave no trace. The teardown wraps the
    // leg BODY, which a rejected guard never enters, so the payload survives
    // into the next leg — which takes it and destroys it exactly once.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "kept" });
                match o {
                    Some(let a) if a.tag == "nope" => print(i"first {a.tag}"),
                    Some(let b) => print(i"second {b.tag}"),
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "second kept\ndrop kept\nafter\n",
    );
}

#[test]
fn b62_a_capture_moved_onward_is_destroyed_once_at_its_destination() {
    // The capture leaves the leg as the match's value, so it is not owned at
    // the leg's end and does not drop there; the `let` it lands in owns it and
    // drops it at ITS scope end. Exactly one destruction, after `after`.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "moved" });
                let out = match o {
                    Some(let r) => r,
                    None => Res { tag = "default" },
                };
                print(i"got {out.tag}");
                print("after");
            }
            "#,
        ),
        "got moved\nafter\ndrop moved\n",
    );
}

#[test]
fn b62_a_capture_passed_by_own_is_destroyed_once_in_the_callee() {
    // `own` moves, so the leg no longer owns it — the callee's own scope-end
    // teardown is the single destruction, and it runs before `after`.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "sunk" });
                match o {
                    Some(let r) => sink(r),
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "sink sunk\ndrop sunk\nafter\n",
    );
}

#[test]
fn b62_a_capture_stored_in_a_struct_is_destroyed_with_the_struct() {
    // A struct literal moves resources in (R5): the leg hands ownership to the
    // aggregate, which is itself a resource by containment and drops the
    // payload at the binding's scope end.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            struct Holder {
                item: Res,
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "stored" });
                let holder = match o {
                    Some(let r) => Holder { item = r },
                    None => Holder { item = Res { tag = "default" } },
                };
                print(i"held {holder.item.tag}");
                print("after");
            }
            "#,
        ),
        "held stored\nafter\ndrop stored\n",
    );
}

#[test]
fn b62_a_capture_returned_out_of_the_function_is_destroyed_by_the_caller() {
    // R4: returns move out, including through a match tail. The leg drops
    // nothing and the caller's binding is the owner.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun unwrap_or_default(own o: Option<Res>): Res {
                match o {
                    Some(let r) => r,
                    None => Res { tag = "default" },
                }
            }
            fun main() {
                let got = unwrap_or_default(Some(Res { tag = "returned" }));
                print(i"got {got.tag}");
                print("after");
            }
            "#,
        ),
        "got returned\nafter\ndrop returned\n",
    );
}

#[test]
fn b62_a_leg_that_does_not_run_destroys_nothing() {
    // The other arm is taken: the payload never existed, and the teardown lives
    // inside the leg body, so nothing runs. (The `None` subject owns nothing.)
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = None;
                match o {
                    Some(let r) => print(i"leg {r.tag}"),
                    None => print("none-arm"),
                }
                print("after");
            }
            "#,
        ),
        "none-arm\nafter\n",
    );
}

#[test]
fn b62_an_early_return_out_of_a_leg_still_destroys_the_capture() {
    // Every exit runs drops (`docs/spec/memory.md`, drop timing): the leg's
    // teardown is a `finally`, so `ret` leaves through it.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun pick(own o: Option<Res>): i32 {
                match o {
                    Some(let r) => {
                        print(i"leg {r.tag}");
                        ret 1;
                    }
                    None => { ret 0; }
                }
                2
            }
            fun main() {
                let got = pick(Some(Res { tag = "early" }));
                print(i"got {got}");
                print("after");
            }
            "#,
        ),
        "leg early\ndrop early\ngot 1\nafter\n",
    );
}

#[test]
fn b62_a_leg_capture_inside_a_loop_is_destroyed_each_iteration() {
    // The capture is declared inside the repeatable interior, so its teardown
    // is per-iteration — the same treatment a resource `let` in a loop body
    // gets, and the reason R8 only polices bindings declared OUTSIDE the loop.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun make(n: i32): Option<Res> {
                Some(Res { tag = i"loop{n}" })
            }
            fun main() {
                for n in [0, 1] {
                    match make(n) {
                        Some(let r) => print(i"leg {r.tag}"),
                        None => print("none"),
                    }
                }
                print("after");
            }
            "#,
        ),
        "leg loop0\ndrop loop0\nleg loop1\ndrop loop1\nafter\n",
    );
}

#[test]
fn b62_the_conditional_teardown_idiom_still_destroys_exactly_once() {
    // The shape `docs/spec/memory.md` names as R7's answer. `drop(c)` moves the
    // capture into the sink, so the leg does not also destroy it — the guard
    // against the obvious over-enrollment.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                mut o: Option<Res> = Some(Res { tag = "taken" });
                match o.take() {
                    Some(let c) => drop(c),
                    None => {}
                }
                print("after");
            }
            "#,
        ),
        "drop taken\nafter\n",
    );
}

#[test]
fn b62_an_is_capture_does_not_enroll_and_is_destroyed_once_by_its_subject() {
    // The ownership split's negative half. `x is Some(let r)` is a TEST, not a
    // consuming match (`is_some`'s body is exactly this, and B60 left it free
    // on a resource), so the subject is loaned and stays the owner. Enrolling
    // the capture here would destroy the payload twice.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "tested" });
                if o is Some(let r) {
                    print(i"is {r.tag}");
                }
                print("after");
            }
            "#,
        ),
        "is tested\nafter\ndrop tested\n",
    );
}

#[test]
fn b62_a_loaned_match_subject_capture_does_not_enroll() {
    // R6's second sentence: matching a loan (`match &x`) inspects without
    // consuming, so the subject keeps ownership and its own scope-end teardown
    // is the single destruction.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "loaned" });
                match &o {
                    Some(let r) => print(i"leg {r.tag}"),
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "leg loaned\nafter\ndrop loaned\n",
    );
}

#[test]
fn b62_a_destructure_capture_is_destroyed_at_its_scope_end() {
    // The same root cause in the `let`-pattern position, which the drop
    // planner's `Destructure` arm recorded as a known leak. `let (r, n) = pair`
    // consumes `pair`, so the capture is the payload's only owner and drops at
    // the enclosing scope's end like any `let`.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let pair = (Res { tag = "destructured" }, 3);
                let (r, n) = pair;
                print(i"pair {r.tag} {n}");
                print("after");
            }
            "#,
        ),
        "pair destructured 3\nafter\ndrop destructured\n",
    );
}

#[test]
fn b62_a_loaned_destructure_capture_does_not_enroll() {
    // `let (r, n) = &pair` loans: `pair` is never consumed, so it stays the
    // owner and the captures must not double-enroll.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let pair = (Res { tag = "borrowed" }, 3);
                let (r, n) = &pair;
                print(i"pair {r.tag} {n}");
                print("after");
            }
            "#,
        ),
        "pair borrowed 3\nafter\ndrop borrowed\n",
    );
}

#[test]
fn b62_a_data_capture_is_untouched_by_the_enrollment() {
    // Data captures are completely unaffected: the program below owns one
    // resource (so drop planning is switched on) and matches a DATA option
    // beside it. Exactly one `try` is emitted — the resource `let`'s — and the
    // data leg gets no teardown at all.
    let js = compile(&b62_program(
        r#"
        fun main() {
            let kept = Res { tag = "kept" };
            let o: Option<i32> = Some(5);
            match o {
                Some(let n) => print(i"leg {n}"),
                None => print("none"),
            }
            print(i"kept {kept.tag}");
        }
        "#,
    ))
    .expect("expected a clean compile");
    assert_eq!(
        js.matches("try").count(),
        1,
        "unexpected teardown in:\n{js}"
    );
}

#[test]
fn b62_an_overwritten_mut_capture_destroys_both_values_once_each() {
    // R2 through a capture: assigning onto a `mut` capture that still owns its
    // payload drops the OLD value at the assignment, and the leg's teardown
    // destroys the new one. Two values, one destruction each.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "old" });
                match o {
                    Some(mut r) => {
                        r = Res { tag = "new" };
                        print(i"leg {r.tag}");
                    }
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "drop old\nleg new\ndrop new\nafter\n",
    );
}

#[test]
fn b62_a_leg_capture_and_a_leg_local_drop_in_reverse_declaration_order() {
    // The capture is declared before the leg's own statements, so its teardown
    // wraps theirs: the local dies first, the capture last — the same nesting
    // `walk_scope_body` gives two consecutive resource `let`s.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun main() {
                let o: Option<Res> = Some(Res { tag = "capture" });
                match o {
                    Some(let r) => {
                        let local = Res { tag = "local" };
                        print(i"leg {r.tag} {local.tag}");
                    }
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "leg capture local\ndrop local\ndrop capture\nafter\n",
    );
}

#[test]
fn b62_a_capture_consumed_by_a_nested_match_is_destroyed_once() {
    // The outer capture is the inner match's subject, so the inner match
    // consumes it and the outer leg drops nothing; the inner capture is the
    // payload's last owner. One destruction, at the inner leg's end.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            resource enum Wrap {
                Inner(Res),
                Nothing,
            }
            fun main() {
                let w: Option<Wrap> = Some(Wrap::Inner(Res { tag = "deep" }));
                match w {
                    Some(let inner) => match inner {
                        Wrap::Inner(let r) => print(i"deep {r.tag}"),
                        Wrap::Nothing => print("nothing"),
                    },
                    None => print("none"),
                }
                print("after");
            }
            "#,
        ),
        "deep deep\ndrop deep\nafter\n",
    );
}

#[test]
fn b62_a_conditionally_moved_capture_is_an_r7_error() {
    // R7 already governed captures — the affine scan seeds them
    // (`seed_pattern_bindings`) so a leg-local move is compared across the
    // branch — and the drop planner's enrollment must not open an escape hatch
    // from it: a capture moved on one path and destroyed on another is exactly
    // the runtime drop flag R7 exists to rule out. This pin guards that the
    // rejection survives the enrollment.
    assert_fails_with(
        &b62_program(
            r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let flag = true;
                let o: Option<Res> = Some(Res { tag = "conditional" });
                match o {
                    Some(let r) => {
                        if flag {
                            sink(r);
                        }
                    }
                    None => print("none"),
                }
            }
            "#,
        ),
        "moved on one path through this branch but not another",
    );
}

// --- B62's residuals: found while pinning the split, each a DIFFERENT rule ----

#[test]
fn b62_an_is_capture_consumed_by_an_own_call_is_rejected() {
    // `is` loans its subject, so the capture is a view into a value the subject
    // still owns. `own`-passing it hands a second owner the payload while the
    // subject's scope-end teardown still fires: the program below printed
    // `sink ic / drop ic / after / drop ic` — one value, TWO destructions.
    //
    // This is B60's rule (a body may only consume what it owns) in the capture
    // position rather than the parameter position, with its own diagnostic and
    // steer ("match by value, or `take`"). CLOSED by B65; the ownership split
    // B62 pins is what makes it a bug: a loaned capture owns nothing, so it may
    // not be consumed.
    assert_fails(&b62_program(
        r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "ic" });
                if o is Some(let r) {
                    sink(r);
                }
                print("after");
            }
            "#,
    ));
}

#[test]
fn b62_a_loaned_match_capture_consumed_by_an_own_call_is_rejected() {
    // The same hole through `match &x`, which R6 defines as inspecting without
    // consuming. Printed `sink lc / drop lc / after / drop lc` before B65.
    assert_fails(&b62_program(
        r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "lc" });
                match &o {
                    Some(let r) => sink(r),
                    None => {}
                }
                print("after");
            }
            "#,
    ));
}

#[test]
fn b62_a_generic_capture_never_moved_out_is_rejected_at_a_resource_instantiation() {
    // R11 requires an `own T` parameter to be moved out on every path, because
    // a generic body is emitted once and cannot destroy a `T`. A pattern
    // capture of generic type is the same situation and was not checked: the
    // match consumes `o` (so the parameter passes the exactly-once test), the
    // capture `v` is never moved out, and the payload leaked — this printed
    // `some / after` with no destruction.
    //
    // CLOSED by B66, which widened `check_own_generic_exactly_once` from `own`
    // parameters to every value the body would have to destroy. Concrete
    // resource captures are unaffected: they are enrolled and destroyed (B62),
    // which is why this was a residual and not a hole in the enrollment.
    assert_fails(&b62_program(
        r#"
            fun peek<type T>(own o: Option<T>) {
                match o {
                    Some(let v) => print("some"),
                    None => print("none"),
                }
            }
            fun main() {
                peek(Some(Res { tag = "gc" }));
                print("after");
            }
            "#,
    ));
}

// === `Task<Task<T>>` assimilation (async-polymorphism.md Part B) =============
//
// A task of a task does not exist at runtime: `Task` is a host thenable, and
// the promise resolution procedure ADOPTS a thenable result instead of boxing
// it, recursively. The type used to sit one level deeper than the value —
// `await` on a nested handle typed `Task<i32>` over a runtime `7`. The handle's
// payload is now assimilated wherever a `Task<..>` is FORMED (the `async` seam
// and generic substitution), which makes `Task<..>` idempotent as a type
// constructor and `await`'s single unwrap exact.

/// The runtime truth this is all measured against: one `await` on a nested task
/// yields the INNERMOST value, not a handle.
#[test]
fn a_nested_task_awaits_to_the_inner_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun main() {
            let inner: Task<i32> = async { 7 };
            let outer = async { inner };
            print(await inner_value(outer));
        }

        fun inner_value(outer: Task<i32>): Task<i32> { outer }
        "#,
        "7\n",
    );
}

/// The construction seam: `async { <a task> }` adds no layer — the handle it
/// yields is a `Task<i32>`, and that is what the runtime holds.
#[test]
fn an_async_body_that_is_a_task_assimilates_at_construction() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun main() {
            let inner: Task<i32> = async { 7 };
            let outer: Task<i32> = async { inner };
            let value: i32 = await outer;
            print(value);
        }
        "#,
        "7\n",
    );
}

/// The typing this closes: `await` on a nested task is an `i32`, matching the
/// value it produces. Before assimilation this was `Task<i32>` — one layer
/// deeper than the runtime.
#[test]
fn awaiting_a_nested_task_types_as_the_inner_value() {
    assert_compiles(
        r#"
        import std::task::Task;

        fun main() {
            let inner: Task<i32> = async { 7 };
            let outer = async { inner };
            let value: i32 = await outer;
        }
        "#,
    );
}

/// The other half, and the user-visible change: the one-layer-deep type is now
/// REJECTED. Nothing produces a `Task<Task<i32>>`, so annotating one is an
/// error rather than a silent lie about the value.
#[test]
fn a_nested_task_no_longer_types_one_layer_deep() {
    assert_fails_with(
        r#"
        import std::task::Task;

        fun main() {
            let inner: Task<i32> = async { 7 };
            let outer = async { inner };
            let value: Task<i32> = await outer;
        }
        "#,
        "Expected Task<i32>, but got i32 instead.",
    );
}

/// No infinite regress, and no residue: a CHAIN of wrapping collapses to one
/// layer, because each construction assimilates as it is formed.
#[test]
fn a_chain_of_nested_tasks_collapses_to_one_layer() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun main() {
            let a: Task<i32> = async { 7 };
            let b: Task<i32> = async { a };
            let c: Task<i32> = async { b };
            let d: Task<i32> = async { c };
            let value: i32 = await d;
            print(value);
        }
        "#,
        "7\n",
    );
}

/// The same chain typed the old way is rejected at every depth — the pin that
/// the collapse is total, not a single-layer trim.
#[test]
fn a_chain_of_nested_tasks_rejects_the_deep_type() {
    assert_fails_with(
        r#"
        import std::task::Task;

        fun main() {
            let a: Task<i32> = async { 7 };
            let b = async { a };
            let c = async { b };
            let value: Task<Task<i32>> = await c;
        }
        "#,
        "Expected Task<Task<i32>>, but got i32 instead.",
    );
}

/// An `async` literal directly inside an `async` literal — the same seam with
/// no intervening binding, so the body type arrives already a handle.
#[test]
fn an_async_literal_body_assimilates() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun main() {
            let outer: Task<i32> = async { async { 7 } };
            let value: i32 = await outer;
            print(value);
        }
        "#,
        "7\n",
    );
}

/// THE SHARP EDGE: a generic `T` that happens to instantiate at a `Task`. The
/// declared return `Task<T>` would substitute to `Task<Task<i32>>` — the very
/// type the host cannot hold — so substitution assimilates too. Verified
/// against the runtime: the program prints the inner value.
#[test]
fn a_generic_wrapper_instantiated_at_a_task_assimilates() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun wrap<T>(t: T): Task<T> { async { t } }

        fun main() {
            let inner: Task<i32> = async { 7 };
            let wrapped: Task<i32> = wrap(inner);
            let value: i32 = await wrapped;
            print(value);
        }
        "#,
        "7\n",
    );
}

/// The generic edge's negative half: the pre-assimilation typing is rejected.
#[test]
fn a_generic_wrapper_at_a_task_rejects_the_deep_type() {
    assert_fails_with(
        r#"
        import std::task::Task;

        fun wrap<T>(t: T): Task<T> { async { t } }

        fun main() {
            let inner: Task<i32> = async { 7 };
            let value: Task<i32> = await wrap(inner);
        }
        "#,
        "Expected Task<i32>, but got i32 instead.",
    );
}

/// The combinators see the assimilated element type, so a join over
/// generically-wrapped tasks yields the values the host actually produces.
#[test]
fn settle_all_over_generically_wrapped_tasks_yields_values() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun wrap<T>(t: T): Task<T> { async { t } }

        fun main() {
            let a: Task<i32> = async { 1 };
            let b: Task<i32> = async { 2 };
            let results: List<i32> = Task::settle_all([wrap(a), wrap(b)]);
            print(results[0] + results[1]);
        }
        "#,
        "3\n",
    );
}

/// The precision guard on the generic seam: the SAME wrapper at a non-task
/// argument is untouched — `wrap(5)` is a `Task<i32>`, one layer, as always.
#[test]
fn a_generic_wrapper_at_a_non_task_is_unaffected() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun wrap<T>(t: T): Task<T> { async { t } }

        fun main() {
            let wrapped: Task<i32> = wrap(5);
            let value: i32 = await wrapped;
            print(value);
        }
        "#,
        "5\n",
    );
}

/// Assimilation is `Task`-specific: every other nesting type constructor keeps
/// its layers, including generic instantiation at its own type.
#[test]
fn non_task_nesting_is_untouched_by_assimilation() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };

        fun hold<T>(value: T): Option<T> { Some(value) }

        fun main() {
            let nested: Option<Option<i32>> = hold(Some(4));
            let lists: List<List<i32>> = [[1], [2]];
            match nested {
                Some(let inner) => match inner {
                    Some(let value) => print(value + lists[1][0]),
                    None => print("none"),
                },
                None => print("none"),
            }
        }
        "#,
        "6\n",
    );
}

/// A plain (non-async) function returning a handle is NOT a nesting site: it
/// hands back the task it built, and the type says so.
#[test]
fn a_plain_function_returning_a_task_still_yields_a_handle() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        fun make(): Task<i32> { async { 7 } }

        fun main() {
            let handle: Task<i32> = make();
            print(await handle);
        }
        "#,
        "7\n",
    );
}

/// RESIDUAL, pinned with the honest CURRENT behavior: an `async fun` whose
/// DECLARED return is itself a `Task`. Its calls are implicitly awaited, so the
/// host assimilates the returned handle and the call site receives the inner
/// `i32` — this program prints `7`, not a handle — while the type still reads
/// `Task<i32>`. The same divergence as the one above, at a seam this fix cannot
/// reach: async-ness is a whole-program fixpoint over the call graph
/// (`async_infer::infer`), computed AFTER type inference, so while a call's type
/// is being decided the analyzer does not yet know whether its callee is async
/// and its result therefore assimilated. Closing it needs the two passes
/// interleaved (or an `Awaited<T>` type-level operator), which is more than this
/// item. Recorded in async-polymorphism.md.
#[test]
fn an_async_function_returning_a_task_is_assimilated_at_runtime_only() {
    // The runtime: the call site receives the VALUE.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::Task;

        async fun make(): Task<i32> { async { 7 } }

        fun main() {
            let result = make();
            print(result);
        }
        "#,
        "7\n",
    );
    // The type: still the handle, one layer deeper than that value.
    assert_compiles(
        r#"
        import std::task::Task;

        async fun make(): Task<i32> { async { 7 } }

        fun main() { let result: Task<i32> = make(); }
        "#,
    );
}

/// The residual's desired end state — `#[ignore]`d until the seam above closes.
/// Un-ignore when an async call's type assimilates its awaited result.
#[test]
#[ignore = "async-fun return assimilation needs the async fixpoint at typing time"]
fn an_async_function_returning_a_task_should_type_as_the_value() {
    assert_compiles(
        r#"
        import std::task::Task;

        async fun make(): Task<i32> { async { 7 } }

        fun main() { let result: i32 = make(); }
        "#,
    );
}

// --- B66: a generic body cannot destroy a `T`, so no delta-resource value may
// reach a scope-end drop (`proposal/affine-moves.md` §9.2) ---------------------

#[test]
fn b66_the_generic_capture_leak_names_the_capture_at_the_instantiation() {
    // C2 for the widened check: primary at the INSTANTIATION (A2 — user code
    // only, never inside the generic), note into the body at the capture.
    assert_fails_spanning(
        &b62_program(
            r#"
            fun peek<type T>(own o: Option<T>) {
                match o {
                    Some(let v) => print("some"),
                    None => print("none"),
                }
            }
            fun main() {
                peek(Some(Res { tag = "gc" }));
            }
            "#,
        ),
        "peek(Some(Res { tag = \"gc\" }))",
        "a resource-typed value still owns its payload where its scope ends",
    );
}

#[test]
fn b66_a_generic_capture_moved_on_is_clean() {
    // The load-bearing negative: the rule is about values the body would have to
    // DESTROY, not about captures. Move the capture onward — here into the
    // returned `Some` — and the same generic is accepted and runs, with the
    // caller destroying the payload exactly once.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun repack<type T>(own o: Option<T>): Option<T> {
                match o {
                    Some(let v) => Some(v),
                    None => None,
                }
            }
            fun main() {
                let back = repack(Some(Res { tag = "moved" }));
                print("after");
            }
            "#,
        ),
        "after\ndrop moved\n",
    );
}

#[test]
fn b66_a_concrete_capture_that_drops_at_its_scope_end_is_untouched() {
    // The regression guard the widening most needs: B62's enrollment is for
    // CONCRETE resource captures, which a monomorphic body destroys perfectly
    // well. This is the same program as the rejected generic, written at a
    // concrete type — it must still compile AND still print the drop. Only a
    // generic body is unable to destroy, so only a generic body is asked.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun peek(own o: Option<Res>) {
                match o {
                    Some(let v) => print(i"some {v.tag}"),
                    None => print("none"),
                }
            }
            fun main() {
                peek(Some(Res { tag = "concrete" }));
                print("after");
            }
            "#,
        ),
        "some concrete\ndrop concrete\nafter\n",
    );
}

#[test]
fn b66_a_generic_local_that_would_drop_at_its_scope_end_is_rejected() {
    // The widening is to `plan_scope`'s whole `dropped` set, not to captures
    // specially — the honest reading of "a generic body cannot destroy a `T`".
    // A `let` local of delta-resource type is the same leak with no pattern in
    // sight, and would have stayed open under a capture-only fix.
    let source = b62_program(
        r#"
            fun stash<type T>(own value: T) {
                let held = value;
            }
            fun main() {
                stash(Res { tag = "local" });
            }
            "#,
    );
    assert_fails_spanning(
        &source,
        "stash(Res { tag = \"local\" })",
        "a resource-typed value still owns its payload where its scope ends",
    );
    // The note names the offending binding and points into the generic body.
    let rejections = r11_rejections(&source);
    assert_eq!(rejections.len(), 1, "one rejection; got: {rejections:#?}");
    let (note_msg, _, _) = rejections[0].2.as_ref().expect("a note into the body");
    assert!(
        note_msg.contains("`held` still owns a value where its scope ends"),
        "the note names the local; got: {note_msg:?}"
    );
}

#[test]
fn b66_a_generic_overwrite_that_would_drop_the_old_value_is_rejected() {
    // The THIRD and last place the drop planner schedules a destruction: R2's
    // overwrite drop. `mut held = a; held = b;` must destroy `a` at the
    // assignment, which a generic body cannot do — before this, `swap` compiled
    // and printed only `drop second`, silently leaking `first`.
    //
    // Found while auditing whether the widening was complete, and closed here
    // rather than filed: the whole point of this arc is that a rule enforced at
    // one position and stated at all of them is how these holes are made.
    let source = r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun swap<T>(own a: T, own b: T): T {
            mut held = a;
            held = b;
            held
        }
        fun main() {
            let out = swap(Db { tag = "first" }, Db { tag = "second" });
            drop(out);
        }
        "#;
    assert_fails_spanning(
        source,
        "swap(Db { tag = \"first\" }, Db { tag = \"second\" })",
        "a resource-typed value is overwritten while it still owns a payload",
    );
    // The note lands on the ASSIGNMENT, not on the binding — that is the line
    // the user has to change, and R2 is named so the rule is findable.
    let rejections = r11_rejections(source);
    assert_eq!(rejections.len(), 1, "one rejection; got: {rejections:#?}");
    let (note_msg, note_range, _) = rejections[0].2.as_ref().expect("a note into the body");
    assert!(
        note_msg.contains("would have to destroy `held`'s previous value (R2)"),
        "the note names the overwritten binding and the rule; got: {note_msg:?}"
    );
    let assignment_at = source.find("held = b").unwrap();
    assert_eq!(
        *note_range,
        assignment_at..assignment_at + "held = b".len(),
        "the note spans the assignment"
    );
}

#[test]
fn b66_a_concrete_overwrite_still_drops_the_old_value() {
    // The guard: R2's overwrite drop is correct and stays correct at a concrete
    // type, where the body CAN destroy. Only a generic body is asked.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut held = Db { tag = "first" };
            held = Db { tag = "second" };
            print("end");
        }
        "#,
        "drop first\nend\ndrop second\n",
    );
}

// --- B101: R2's overwrite through a place the generic body does not own ------
// B94 and B99 made a write THROUGH a `&mut` and a write OVER a component
// destroy what they replace. Both drops are per-type glue a generic body cannot
// emit, so at a resource instantiation the body owes a destruction it cannot
// run — and `check_own_generic_exactly_once` did not look: its `place_overwrites`
// was deliberately empty, with the reason recorded in the code. It looks now,
// at the per-instantiation DELTA place set, so a concrete resource written
// inside a generic body stays chunk 3's report. See proposal/destruction.md R11.

/// The B101 pin family's shared preamble.
fn b101_program(body: &str) -> String {
    format!(
        r#"
        import std::print;
        import std::drop::{{ Drop, drop }};
        import std::option::Option::{{ self, Some, None }};
        resource struct Guard {{ label: str }}
        impl Guard with Drop {{ fun drop(&mut self) {{ print(i"dropped {{self.label}}"); }} }}
        {body}
        "#
    )
}

#[test]
fn b101_a_generic_write_through_a_mut_t_is_rejected() {
    // The filed shape. `slot = value` inside `&mut T` is B94's write: it
    // destroys the pointee's outgoing value, which at `T := Guard` is a `Guard`
    // the shared body cannot destroy. Before this it compiled and printed
    // "before\nafter\ndropped second" — "first" leaked outright.
    let source = b101_program(
        r#"
        fun set<T>(slot: &mut T, own value: T) { slot = value; }
        fun main() {
            mut g = Guard { label = "first" };
            set(&mut g, Guard { label = "second" });
        }
        "#,
    );
    assert_fails_spanning(
        &source,
        "set(&mut g, Guard { label = \"second\" })",
        "a resource-typed value is overwritten while it still owns a payload",
    );
    let rejections = r11_rejections(&source);
    assert_eq!(rejections.len(), 1, "one rejection; got: {rejections:#?}");
    let (note_msg, note_range, _) = rejections[0].2.as_ref().expect("a note into the body");
    assert!(
        note_msg.contains("would have to destroy `slot`'s previous value (R2)"),
        "the note names the written place and the rule; got: {note_msg:?}"
    );
    let assignment_at = source.find("slot = value").unwrap();
    assert_eq!(
        *note_range,
        assignment_at..assignment_at + "slot = value".len(),
        "the note spans the assignment"
    );
}

#[test]
fn b101_a_generic_write_through_a_reborrow_is_rejected() {
    // The same write under a second name. `binding_or_param_is_view` follows the
    // copy chain, so a local bound to a `&mut` of the parameter is a loan too —
    // which is what keeps the question at the place rather than at the
    // parameter list.
    let source = b101_program(
        r#"
        fun set<T>(slot: &mut T, own value: T) {
            let inner = &mut slot;
            inner = value;
        }
        fun main() {
            mut g = Guard { label = "first" };
            set(&mut g, Guard { label = "second" });
        }
        "#,
    );
    assert_fails_spanning(
        &source,
        "set(&mut g, Guard { label = \"second\" })",
        "a resource-typed value is overwritten while it still owns a payload",
    );
    let rejections = r11_rejections(&source);
    let (note_msg, _, _) = rejections[0].2.as_ref().expect("a note into the body");
    assert!(
        note_msg.contains("would have to destroy `inner`'s previous value (R2)"),
        "the note names the re-borrow; got: {note_msg:?}"
    );
}

#[test]
fn b101_a_generic_write_over_a_component_is_rejected() {
    // B99's arm inside a generic body: `holder.item = value` overwrites a
    // `T`-typed component, so the same drop is owed and the same body cannot
    // emit it. The note names the ROOT the component sits in, because the
    // component itself names no value of its own.
    let source = b101_program(
        r#"
        struct Wrap<type T> { item: T }
        fun set<T>(holder: &mut Wrap<T>, own value: T) { holder.item = value; }
        fun main() {
            mut w = Wrap { item = Guard { label = "first" } };
            set(&mut w, Guard { label = "second" });
        }
        "#,
    );
    assert_fails_spanning(
        &source,
        "set(&mut w, Guard { label = \"second\" })",
        "a resource-typed value is overwritten while it still owns a payload",
    );
    let rejections = r11_rejections(&source);
    assert_eq!(rejections.len(), 1, "one rejection; got: {rejections:#?}");
    let (note_msg, note_range, _) = rejections[0].2.as_ref().expect("a note into the body");
    assert!(
        note_msg.contains("would have to destroy the value it replaces inside `holder` (R2)"),
        "the note names the root place and the rule; got: {note_msg:?}"
    );
    let assignment_at = source.find("holder.item = value").unwrap();
    assert_eq!(
        *note_range,
        assignment_at..assignment_at + "holder.item = value".len(),
        "the note spans the assignment"
    );
}

#[test]
fn b101_a_generic_overwrite_with_no_own_parameter_is_rejected() {
    // The check used to return early unless the body took an `own T`, which was
    // its original scope surviving as a guard. `clear` declares none and owes
    // R2's drop anyway — before this it compiled and leaked the payload without
    // printing anything.
    let source = b101_program(
        r#"
        fun clear<T>(slot: &mut Option<T>) { slot = None; }
        fun main() {
            mut o = Some(Guard { label = "first" });
            clear(&mut o);
        }
        "#,
    );
    assert_fails_spanning(
        &source,
        "clear(&mut o)",
        "a resource-typed value is overwritten while it still owns a payload",
    );
}

#[test]
fn b101_a_generic_scope_end_drop_with_no_own_parameter_is_rejected() {
    // The same guard removal reaches B66's other half: `taken` is a
    // delta-resource local still owning where its scope ends, in a body that
    // takes no `own T` at all. One rule, asked wherever it applies.
    let source = b101_program(
        r#"
        fun stash<T>(slot: &mut Option<T>) { let taken = slot.take(); }
        fun main() {
            mut o = Some(Guard { label = "first" });
            stash(&mut o);
        }
        "#,
    );
    assert_fails_spanning(
        &source,
        "stash(&mut o)",
        "a resource-typed value still owns its payload where its scope ends",
    );
}

#[test]
fn b101_a_concrete_instantiation_of_the_same_body_is_accepted() {
    // The negative that keeps the rule about resources: `T := i32` is not a
    // resource instantiation, nothing is enqueued, and the same body compiles
    // and runs.
    assert_compiles_and_runs(
        &b101_program(
            r#"
        fun set<T>(slot: &mut T, own value: T) { slot = value; }
        fun main() {
            mut n = 1;
            set(&mut n, 2);
            print(n);
        }
        "#,
        ),
        "2\n",
    );
}

#[test]
fn b101_a_concrete_body_with_the_same_write_still_drops() {
    // The other negative, and the reason the rule is R11's rather than R2's: a
    // CONCRETE `&mut Guard` body knows the type, so B94's drop is emitted and
    // the write is correct. Only a shared generic body is asked.
    assert_compiles_and_runs(
        &b101_program(
            r#"
        fun set(slot: &mut Guard, own value: Guard) { slot = value; }
        fun main() {
            mut g = Guard { label = "first" };
            print("before");
            set(&mut g, Guard { label = "second" });
            print("after");
        }
        "#,
        ),
        "before\ndropped first\nafter\ndropped second\n",
    );
}

#[test]
fn b101_a_concrete_resource_written_inside_a_generic_body_is_not_r11s() {
    // Why the question is asked of the DELTA place set and not of concrete
    // resource-ness. `slot.held` is a `Guard` at every instantiation, so the
    // emitted body knows the type and B99's drop fires — this is chunk 3's
    // territory, and re-asking it here would reject a correct program once per
    // instantiation site. Runs, and destroys all three in order.
    assert_compiles_and_runs(
        &b101_program(
            r#"
        resource struct Slot { held: Guard }
        fun bump<T>(own value: T, slot: &mut Slot): T {
            slot.held = Guard { label = "fresh" };
            value
        }
        fun main() {
            mut s = Slot { held = Guard { label = "old" } };
            let g = bump(Guard { label = "passed" }, &mut s);
            drop(g);
            print("end");
        }
        "#,
        ),
        "dropped old\ndropped passed\nend\ndropped fresh\n",
    );
}

#[test]
fn b101_a_read_only_generic_view_body_is_accepted() {
    // A `&T` body that never writes owes nothing, so the widened check must not
    // reach it — the predicate is the WRITE, not the loan.
    assert_compiles_and_runs(
        &b101_program(
            r#"
        fun peek<T>(slot: &Option<T>): bool { slot.is_some() }
        fun main() {
            let o = Some(Guard { label = "first" });
            print(peek(&o));
        }
        "#,
        ),
        "true\ndropped first\n",
    );
}

#[test]
fn b101_the_option_surface_still_instantiates_at_a_resource() {
    // The std sweep, as a pin: `Option` is the sanctioned resource container
    // (R10), and its generic surface must stay clean under the widening.
    // `take`, `replace`, `unwrap`, `is_some`/`is_none` and the `drop` sink are
    // every generic a resource can reach in std today.
    assert_compiles_and_runs(
        &b101_program(
            r#"
        fun main() {
            mut slot = Some(Guard { label = "a" });
            print(slot.is_some());
            match slot.take() {
                Some(let g) => drop(g),
                None => {},
            }
            print(slot.is_none());
            mut refilled = Some(Guard { label = "b" });
            match refilled.replace(Guard { label = "c" }) {
                Some(let g) => drop(g),
                None => {},
            }
            drop(refilled.unwrap());
        }
        "#,
        ),
        "true\ndropped a\ntrue\ndropped b\ndropped c\n",
    );
}

#[test]
fn b66_a_body_that_already_failed_the_move_scan_reports_once() {
    // B5 — one diagnostic per root cause. The drop plan assumes an affine-valid
    // body; when the move scan already reported, the leftover ownership is a
    // CONSEQUENCE of that failure, not a second problem. `keep` still owns only
    // because `x` was used twice, which is already the error.
    let source = r#"
        resource struct Db { handle: i32 }
        fun use_twice<T>(own x: T): T {
            let keep = x;
            x
        }
        fun sink(own d: Db) {}
        fun main() {
            let db = Db { handle = 1 };
            let out = use_twice(db);
            sink(out);
        }
        "#;
    let rejections = r11_rejections(source);
    assert_eq!(
        rejections.len(),
        1,
        "the double use is the one root cause; got: {rejections:#?}"
    );
    assert!(
        rejections[0].0.contains("used more than once"),
        "and it is the move violation, not the drop-plan consequence; got: {rejections:#?}"
    );
}

// --- B65: a capture of a LOANED subject is a loan, and may not be consumed
// (`proposal/affine-moves.md` §9.1) -------------------------------------------

#[test]
fn b65_the_is_capture_diagnostic_names_the_subject_and_the_by_value_steer() {
    // C2: the diagnostic class carries its span + message pin. The steer is
    // deliberately NOT `LoanConsumed`'s "declare it `own x`" — a capture has no
    // convention to redeclare, so the fix is to consume the SUBJECT. It also
    // offers no copy: vilan has no user-facing copy spelling, and R1 forbids
    // copying a resource at all, so "clone the payload" would name an
    // impossible fix (diagnostics-standard B4).
    assert_fails_spanning_nth(
        &b62_program(
            r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "ic" });
                if o is Some(let held) {
                    sink(held);
                }
            }
            "#,
        ),
        // Occurrence 0 binds the capture; the diagnostic anchors the CONSUMING
        // use (A1 — the narrowest span that identifies the problem).
        "held",
        1,
        "cannot move the resource `held` out of this pattern: it captures from `o`, \
         which is matched by loan",
    );
}

#[test]
fn b65_the_loaned_match_capture_diagnostic_steers_to_the_by_value_match() {
    // The `match &o` form reaches the same rule through the same subject name:
    // `pattern_subject_name` looks through the `&`, so the steer says `match o`,
    // the spelling that actually fixes it.
    assert_fails_spanning_nth(
        &b62_program(
            r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "lc" });
                match &o {
                    Some(let held) => sink(held),
                    None => {}
                }
            }
            "#,
        ),
        "held",
        1,
        "match `o` by value to move the payload into the capture, or restructure \
         with `Option` + `take`",
    );
}

#[test]
fn b65_a_loaned_capture_that_is_only_read_stays_legal() {
    // The load-bearing half: B65 rejects CONSUMING a loaned capture, never
    // reading one. `is`-testing and reading the payload through the capture is
    // the idiom `is_some_and` / `inspect` are built on (B63 §8.3), and the
    // subject keeps ownership and drops exactly once at its own scope end.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun peek(r: &Res) {
                print(i"peek {r.tag}");
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "read" });
                if o is Some(let r) {
                    print(i"is {r.tag}");
                    peek(&r);
                }
                match &o {
                    Some(let r) => print(i"leg {r.tag}"),
                    None => {}
                }
                print("after");
            }
            "#,
        ),
        "is read\npeek read\nleg read\nafter\ndrop read\n",
    );
}

#[test]
fn b65_the_consuming_match_form_still_moves_its_capture_onward() {
    // The steer's own target must keep working: `match o` by value consumes the
    // subject, so the capture OWNS the payload (B62) and `own`-passing it is a
    // legal move, not a second owner. One `sink`, one `drop`, and no teardown
    // of `o` — this is what B65 steers users toward, so it is pinned beside it.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let o: Option<Res> = Some(Res { tag = "owned" });
                match o {
                    Some(let r) => sink(r),
                    None => {}
                }
                print("after");
            }
            "#,
        ),
        "sink owned\ndrop owned\nafter\n",
    );
}

#[test]
fn b65_a_loaned_destructure_capture_consumed_is_rejected() {
    // The third loan form in §7.2's table, and the twin B62 fixed on the
    // enrollment side: `let (a, b) = &pair` loans, so its captures own nothing.
    // Pinned because a fix aimed only at `match`/`is` would leave it open.
    assert_fails_spanning_nth(
        &b62_program(
            r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let pair = (Res { tag = "d0" }, 1);
                let (held, n) = &pair;
                sink(held);
            }
            "#,
        ),
        "held",
        1,
        "cannot move the resource `held` out of this pattern: it captures from \
         `pair`, which is matched by loan",
    );
}

#[test]
fn b65_a_consuming_destructure_capture_is_still_movable() {
    // The consuming twin of the above — `let (r, n) = pair` consumes `pair`, so
    // `r` owns and may be moved on. Guards the destructure half against an
    // over-wide fix that treated every destructure capture as a loan.
    assert_compiles_and_runs(
        &b62_program(
            r#"
            fun sink(own r: Res) {
                print(i"sink {r.tag}");
            }
            fun main() {
                let pair = (Res { tag = "d1" }, 1);
                let (r, n) = pair;
                sink(r);
                print("after");
            }
            "#,
        ),
        "sink d1\ndrop d1\nafter\n",
    );
}

#[test]
fn b65_a_loaned_capture_consumed_inside_a_generic_reports_at_the_instantiation() {
    // B65 rides R11's per-instantiation scan unchanged, like every other rule in
    // `scan_move` — the same predicate, the same `MoveScan`, no new plumbing.
    // The report lands at the INSTANTIATION site (A2: user code only), with the
    // note pointing into the generic body.
    //
    // `keep` is what makes the consuming use real: a closure-valued callee loans
    // every argument (`callee_conventions` answers `None`), so only a resolvable
    // `own` callee consumes. `steal` returns `o`, so R11's exactly-once check is
    // satisfied and this is the ONE diagnostic.
    assert_fails_spanning(
        &b62_program(
            r#"
            fun keep<type T>(own v: T): T {
                v
            }
            fun steal<type T>(own o: Option<T>): Option<T> {
                if o is Some(let held) {
                    keep(held);
                }
                o
            }
            fun main() {
                let kept = steal(Some(Res { tag = "gi" }));
            }
            "#,
        ),
        "steal(Some(Res { tag = \"gi\" }))",
        "a capture of a loaned resource-typed subject is moved out",
    );
}

// --- B57: method-resolution precedence (proposal/method-resolution.md) -------

/// The rule itself: an inherent member outranks a trait's, whatever order the
/// impl blocks are written in. The TRAIT block comes first here, so the old
/// first-registration-wins scan answered `TRAIT`.
#[test]
fn b57_an_inherent_method_outranks_a_trait_method() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait Iter<T> { fun pick(self): str; }
        impl Bag with Iter<i32> { fun pick(self): str { "TRAIT" } }
        impl Bag { fun pick(self): str { "INHERENT" } }
        fun main() { print(Bag { x = 1 }.pick()); }
        "#,
        "INHERENT\n",
    );
}

/// …and the same answer with the blocks swapped. Precedence is a property of
/// the program, not of where its text happens to sit (§3(c)'s whole argument
/// against blessing declaration order).
#[test]
fn b57_inherent_precedence_does_not_depend_on_impl_order() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait Iter<T> { fun pick(self): str; }
        impl Bag { fun pick(self): str { "INHERENT" } }
        impl Bag with Iter<i32> { fun pick(self): str { "TRAIT" } }
        fun main() { print(Bag { x = 1 }.pick()); }
        "#,
        "INHERENT\n",
    );
}

/// A trait member with no inherent competitor still resolves — the tiering
/// ranks, it does not hide.
#[test]
fn b57_a_trait_method_still_resolves_without_an_inherent_one() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait Iter<T> { fun pick(self): str; }
        impl Bag with Iter<i32> { fun pick(self): str { "TRAIT" } }
        fun main() { print(Bag { x = 1 }.pick()); }
        "#,
        "TRAIT\n",
    );
}

/// The precedence carries through a generic subject: `impl Bag<type T>`'s own
/// method wins over the trait impl for the same subject, and the body still
/// monomorphizes against the receiver.
#[test]
fn b57_inherent_precedence_holds_for_a_generic_subject() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag<T> { x: T }
        trait Iter<T> { fun pick(self): T; }
        impl Bag<type T> with Iter<T> { fun pick(self): T { print("TRAIT"); self.x } }
        impl Bag<type T> { fun pick(self): T { print("INHERENT"); self.x } }
        fun main() { print(Bag { x = 7 }.pick()); }
        "#,
        "INHERENT\n7\n",
    );
}

/// Two inherent declarations of one name for one subject: a definition-site
/// error, anchored at the SECOND, with a note at the first (§4). This is the
/// shape the survey found live in the corpus (`vilan/test/gap-b.vl`'s dead
/// `unzip`, shadowed by std's).
#[test]
fn b57_two_inherent_declarations_of_one_name_are_an_error() {
    assert_fails_spanning_nth(
        r#"
        struct Bag { x: i32 }
        impl Bag { fun pick(self): str { "one" } }
        impl Bag { fun pick(self): str { "two" } }
        fun main() { let bag = Bag { x = 1 }; }
        "#,
        // The SECOND declaration's name is the anchor (A1/A3).
        "pick",
        1,
        "is already defined for 'Bag'",
    );
}

/// …and the note points at the first declaration, so "already defined" says
/// where (C3).
#[test]
fn b57_the_duplicate_inherent_error_notes_the_first_declaration() {
    assert_fails_noting(
        r#"
        struct Bag { x: i32 }
        impl Bag { fun first_here(self): str { "one" } }
        impl Bag { fun first_here(self): str { "two" } }
        fun main() { let bag = Bag { x = 1 }; }
        "#,
        "is already defined for 'Bag'",
        "first_here",
        "is already defined here",
    );
}

/// The duplicate rule is about the type's OWN members, so a `with`-clause block
/// declaring a name its traits do not declare is inherent too — and collides
/// with a plain inherent block's same name.
#[test]
fn b57_an_extra_method_in_a_trait_impl_block_counts_as_inherent() {
    assert_fails_spanning_nth(
        r#"
        struct Bag { x: i32 }
        trait Iter<T> { fun step(self): str; }
        impl Bag with Iter<i32> {
            fun step(self): str { "step" }
            fun extra(self): str { "in the with-block" }
        }
        impl Bag { fun extra(self): str { "inherent" } }
        fun main() { let bag = Bag { x = 1 }; }
        "#,
        "extra",
        1,
        "is already defined for 'Bag'",
    );
}

/// Two impls of DIFFERENT subjects sharing a method name are not duplicates —
/// the rule is per subject, not per name.
#[test]
fn b57_the_same_method_name_on_two_types_is_not_a_duplicate() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        struct Box { y: i32 }
        impl Bag { fun pick(self): str { "bag" } }
        impl Box { fun pick(self): str { "box" } }
        fun main() { print(Bag { x = 1 }.pick()); print(Box { y = 2 }.pick()); }
        "#,
        "bag\nbox\n",
    );
}

/// Two traits providing one name, with no inherent member above them: an
/// ambiguity error that names both homes and both disambiguating spellings,
/// built from the call's own receiver (§4).
#[test]
fn b57_two_traits_providing_one_name_are_ambiguous() {
    assert_fails_spanning_nth(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait A { fun pick(self): str; }
        trait B { fun pick(self): str; }
        impl Bag with A { fun pick(self): str { "A" } }
        impl Bag with B { fun pick(self): str { "B" } }
        fun main() { let bag = Bag { x = 1 }; print(bag.pick()); }
        "#,
        "pick",
        4,
        "'pick' is ambiguous on 'Bag': both 'A' and 'B' provide it; \
         call 'A::pick(bag)' or 'B::pick(bag)' to pick one",
    );
}

/// The same rule one tier down: two traits whose DEFAULTS share a name are as
/// ambiguous as two that declare it (§3, S2's third scan).
#[test]
fn b57_two_inherited_defaults_of_one_name_are_ambiguous() {
    assert_fails_spanning_nth(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait A { fun tag(self): str; fun pick(self): str { "from-A" } }
        trait B { fun mark(self): str; fun pick(self): str { "from-B" } }
        impl Bag with A { fun tag(self): str { "t" } }
        impl Bag with B { fun mark(self): str { "m" } }
        fun main() { let bag = Bag { x = 1 }; print(bag.pick()); }
        "#,
        "pick",
        2,
        "'pick' is ambiguous on 'Bag': both 'A' and 'B' provide it",
    );
}

/// A supertrait offering a member through two routes is ONE candidate, not an
/// ambiguity: `Ord with Eq` must not double-count `eq`.
#[test]
fn b57_a_supertrait_reached_twice_is_not_ambiguous() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait Base { fun tag(self): str { "base" } }
        trait Left with Base { fun l(self): str; }
        trait Right with Base { fun r(self): str; }
        impl Bag with Left { fun l(self): str { "l" } }
        impl Bag with Right { fun r(self): str { "r" } }
        fun main() { print(Bag { x = 1 }.tag()); }
        "#,
        "base\n",
    );
}

/// And the third scan: a `T: A + B` bound list whose two arms both supply the
/// name — the same disease in a third location (§8(e)), now the same error.
#[test]
fn b57_a_bound_list_supplying_one_name_twice_is_ambiguous() {
    assert_fails_spanning_nth(
        r#"
        import std::print;
        trait A { fun pick(self): str; }
        trait B { fun pick(self): str; }
        fun through<T: A + B>(value: T): str { value.pick() }
        struct Bag { x: i32 }
        impl Bag with A { fun pick(self): str { "A" } }
        impl Bag with B { fun pick(self): str { "B" } }
        fun main() { print(through(Bag { x = 1 })); }
        "#,
        "pick",
        2,
        "'pick' is ambiguous on 'T': both 'A' and 'B' provide it; \
         call 'A::pick(value)' or 'B::pick(value)' to pick one",
    );
}

/// A bound list whose arms reach the SAME declaration (`T: Sub + Super`) is one
/// candidate, not an ambiguity.
#[test]
fn b57_a_bound_list_reaching_one_declaration_twice_is_not_ambiguous() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait Base { fun tag(self): str; }
        trait Extra with Base { fun more(self): str; }
        fun show<T: Base + Extra>(value: T): str { value.tag() }
        struct Bag { x: i32 }
        impl Bag with Extra { fun tag(self): str { "t" } fun more(self): str { "m" } }
        fun main() { print(show(Bag { x = 1 })); }
        "#,
        "t\n",
    );
}

/// The disambiguator, on an AMBIGUOUS receiver: naming the trait picks that
/// trait's version, and the two spellings give different answers.
#[test]
fn b57_trait_qualified_calls_disambiguate_two_traits() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait A { fun pick(self): str; }
        trait B { fun pick(self): str; }
        impl Bag with A { fun pick(self): str { "A" } }
        impl Bag with B { fun pick(self): str { "B" } }
        fun main() { let bag = Bag { x = 1 }; print(A::pick(bag)); print(B::pick(bag)); }
        "#,
        "A\nB\n",
    );
}

/// The disambiguator, on an INHERENT-SHADOWED receiver: the trait's version is
/// reachable even though the inherent one outranks it at `bag.pick()`. Without
/// this the trait member would be unreachable, not merely outranked.
#[test]
fn b57_a_trait_qualified_call_reaches_past_an_inherent_method() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait Iter<T> { fun pick(self): str; }
        impl Bag with Iter<i32> { fun pick(self): str { "TRAIT" } }
        impl Bag { fun pick(self): str { "INHERENT" } }
        fun main() {
            let bag = Bag { x = 1 };
            print(bag.pick());
            print(Iter::pick(bag));
            print(Bag::pick(bag));
        }
        "#,
        "INHERENT\nTRAIT\nINHERENT\n",
    );
}

/// `ConcreteType::member(receiver)` means the type's OWN member or nothing
/// (§3.1(2)): it must not fall through to a trait's, which would leave a second
/// unreformed door into the ambiguity the rule closes.
#[test]
fn b57_a_type_qualified_call_does_not_fall_through_to_a_trait() {
    assert_fails_spanning(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait A { fun pick(self): str; }
        impl Bag with A { fun pick(self): str { "A" } }
        fun main() { print(Bag::pick(Bag { x = 1 })); }
        "#,
        "Bag::pick",
        "'pick' is not an inherent member of 'Bag': 'A' provides it; \
         call 'A::pick(..)' instead",
    );
}

/// The disambiguator against an INHERITED DEFAULT: the impl declares nothing,
/// so the call re-dispatches on the named trait's surface — which is what keeps
/// two same-named defaults distinguishable.
#[test]
fn b57_a_trait_qualified_call_picks_between_inherited_defaults() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait A { fun tag(self): str; fun pick(self): str { "from-A" } }
        trait B { fun mark(self): str; fun pick(self): str { "from-B" } }
        impl Bag with A { fun tag(self): str { "t" } }
        impl Bag with B { fun mark(self): str { "m" } }
        fun main() { let bag = Bag { x = 1 }; print(A::pick(bag)); print(B::pick(bag)); }
        "#,
        "from-A\nfrom-B\n",
    );
}

/// The disambiguator on a GENERIC receiver: inside `fun f<T: A>` there is no
/// implementation to point at until the call monomorphizes, so it rides the
/// bound-dispatch channel — and still reaches the named trait's version.
#[test]
fn b57_a_trait_qualified_call_works_on_a_generic_receiver() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait A { fun pick(self): str; }
        trait B { fun pick(self): str; }
        impl Bag with A { fun pick(self): str { "A" } }
        impl Bag with B { fun pick(self): str { "B" } }
        fun left<T: A>(value: T): str { A::pick(value) }
        fun right<T: B>(value: T): str { B::pick(value) }
        fun main() { print(left(Bag { x = 1 })); print(right(Bag { x = 1 })); }
        "#,
        "A\nB\n",
    );
}

/// A trait-qualified call with arguments, nested inside another call and
/// chained off the result — the multi-parameter / nested / chained edge forms.
#[test]
fn b57_a_trait_qualified_call_takes_arguments_and_chains() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { x: i32 }
        trait A { fun join(self, left: str, right: str): str; }
        impl Bag with A { fun join(self, left: str, right: str): str { left + right } }
        impl Bag { fun join(self, left: str, right: str): str { "inherent" } }
        impl str { fun shout(self): str { self + "!" } }
        fun main() {
            let bag = Bag { x = 1 };
            print(A::join(bag, "a", "b").shout());
            print(A::join(bag, A::join(bag, "n", "e"), "sted"));
        }
        "#,
        "ab!\nnested\n",
    );
}

/// Naming a trait the receiver does not implement is an error that says so,
/// rather than resolving to the trait's abstract declaration and emitting an
/// empty function.
#[test]
fn b57_a_trait_qualified_call_rejects_an_unimplementing_receiver() {
    assert_fails_spanning(
        r#"
        import std::print;
        struct Bag { x: i32 }
        struct Box { y: i32 }
        trait A { fun pick(self): str; }
        impl Bag with A { fun pick(self): str { "A" } }
        fun main() { print(A::pick(Box { y = 2 })); }
        "#,
        "Box { y = 2 }",
        "does not implement 'A'",
    );
}

// --- B72 → B4 §12.2: a bare trait is refused at the DECLARATION
// --- (method-resolution.md §10, trait-objects.md §11–§12) -------------------
//
// `fun show(v: A)` called with a `Bag` that implements `A` failed with
// "Expected A, but got Bag instead" — which reads as though the impl were
// missing. It is not. A trait is a BOUND, not a type (`spec/types.md` §5.5,
// §5.11) and vilan has no trait objects, so the parameter can never accept a
// concrete value and the declaration is what has to change.
//
// B72 said so at the CALL, because a call was the one position that reconciled
// PARAMETER-FIRST and so the one position that ever asked
// `reconcile_type(Trait, Concrete)` — the direction with no arm. Every other
// position reconciled value-first, landed on the `(Struct|Enum, Trait)` arm and
// ACCEPTED: four of six positions took a bare trait silently, and those four
// acceptances were pinned below as the standing inconsistency they were, filed
// as B4's to settle.
//
// B4 settled it (`proposal/trait-objects.md`, trait objects DECLINED
// 2026-08-07; §11–§12 ship instead). The steer moved from the call to the
// DECLARATION, which is where the fix goes and where it fires whether or not
// anyone ever calls the thing — so the four acceptance pins are inverted here,
// which is the arc closing rather than a regression, and the steer's own pins
// now read at the declaration. What survives verbatim is the register: name the
// rule, then name the declaration that works.

#[test]
fn b72_a_bare_trait_parameter_steers_to_a_bound_generic() {
    // The filed shape. Now refused where it is written, not where it is called.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show(v: A): str { "x" }
        fun main() { let s = show(Bag { n = 1 }); }
        "#,
        "'A' is a trait, not a type: a trait is not a value type",
    );
}

#[test]
fn b72_the_bare_trait_steer_names_the_generic_to_write() {
    // The actionable half — without it the message diagnoses without directing.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show(v: A): str { "x" }
        fun main() { let s = show(Bag { n = 1 }); }
        "#,
        "`<T: A>` — and write 'T' here",
    );
}

#[test]
fn b72_the_bare_trait_refusal_notes_the_trait_declaration() {
    // B72 anchored at the call and needed a note to reach the parameter that
    // had to change. The refusal anchors at that parameter, so the note points
    // at the other thing the reader may not be able to see — the trait — and
    // carries its own source, so it renders when the trait lives in another
    // module (the B72 mechanism, pointed one hop further out).
    assert_fails_noting(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show(subject: A): str { "x" }
        fun main() { let s = show(Bag { n = 1 }); }
        "#,
        "'A' is a trait, not a type",
        "A",
        "is declared here, as a trait",
    );
}

#[test]
fn b72_a_bare_trait_parameter_on_a_static_steers_too() {
    // The second surface B72 had to reach separately — an associated function
    // called as `Type::member(..)` — needs no separate reach now: both are the
    // same written parameter, refused once at the declaration.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        struct Holder { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        impl Holder { fun make(v: A): i32 { 1 } }
        fun main() { let n = Holder::make(Bag { n = 1 }); }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b72_the_refusal_does_not_wait_for_an_argument() {
    // B72's steer was conditional on the argument implementing the trait: at a
    // call, a non-implementing value made the missing impl the likelier
    // mistake, so the plain mismatch stayed the better report. A definition-site
    // rule has no such branch and needs none — `fun show(v: A)` is wrong on its
    // own terms, before any argument exists, and reports identically whether
    // the value passed implements `A` or not. That is what makes it one rule
    // rather than a message.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        struct Other { m: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show(v: A): str { "x" }
        fun main() { let s = show(Other { m = 1 }); }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b72_an_uncalled_bare_trait_parameter_is_still_refused() {
    // The half a use-site steer structurally could not reach: a declaration
    // nobody calls. B72 was silent here; the rule is not.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show(v: A): str { "x" }
        fun main() { }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b72_the_bound_generic_form_is_what_works() {
    // The steer has to point at something that compiles AND runs — otherwise it
    // is advice, not a fix. This is the program the message asks for.
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show<T: A>(v: T): str { v.name() }
        fun main() { print(show(Bag { n = 1 })); }
        main();
        "#,
        "bag\n",
    );
}

#[test]
fn b72_a_generic_parameter_is_untouched_by_the_steer() {
    // The steer must not reach a GENERIC parameter whose constraint resolves to
    // a trait — `Type::Generic(c)` where `c` is the bound's type id is the
    // normal, working case, and it is one arm away in `reconcile_type`.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        struct Other { m: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show<T: A>(v: T): str { v.name() }
        fun main() { let s = show(Other { m = 1 }); }
        "#,
        "does not implement trait 'A'",
    );
}

// The four positions that used to ACCEPT a bare trait, inverted. Each
// reconciled value-first and landed on `reconcile_type`'s `(Struct|Enum, Trait)`
// arm, which returns the CONCRETE side — so the trait was absorbed at the check
// and then reasserted by the annotation, which is how a value ended up carrying
// a type it had no implementation for. They are refused at the annotation now,
// by one rule rather than four checks (`trait-objects.md` §11's table). std's
// own dependency on the return case — `iterator.vl`'s
// `fun iter(self): Iterator<T>` — was answered first, by spelling those five
// declarations `Self`, which is what they always meant (§1.5).

#[test]
fn b72_a_bare_trait_let_annotation_is_refused() {
    // `let x: A = bag` used to compile, and only USING it failed. The spec has
    // said since it was written that the ANNOTATION is the error
    // (`types.md` §5.11); the compiler agrees now.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun main() { let x: A = Bag { n = 1 }; }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b72_a_bare_trait_value_still_cannot_be_used() {
    // The refusal that always existed. The binding now fails one step earlier,
    // at its annotation, so the use never gets its own report — but the rule
    // the reader is told is word-for-word the one `MethodLookup::BareTraitValue`
    // states, which is the point of wording them alike.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun main() { let x: A = Bag { n = 1 }; let s = x.name(); }
        "#,
        "a trait is not a value type (vilan has no trait objects)",
    );
}

#[test]
fn b72_a_bare_trait_method_parameter_is_refused() {
    // A METHOD's bare-trait parameter reconciled value-first, so it accepted
    // where the free function refused. The asymmetry is gone: both are written
    // parameters, and the rule is on the writing.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        struct Holder { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        impl Holder { fun take(self, v: A): i32 { 1 } }
        fun main() { let h = Holder { n = 0 }; let n = h.take(Bag { n = 1 }); }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b72_a_bare_trait_return_is_refused() {
    // The position std itself used, and the reason §11 sequenced the `Self`
    // rewrites before the tightening.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun make(): A { Bag { n = 1 } }
        fun main() { let v = make(); }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b72_a_bare_trait_field_is_refused() {
    // The fifth position — the one §2.2's resource leak rode in on.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        struct Holder { item: A }
        fun main() { let h = Holder { item = Bag { n = 1 } }; }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b72_a_bare_trait_generic_argument_is_refused() {
    // §2.3: `List<A>` type-checked, and then narrowed to `List<Bag>` at the
    // first element because the `(Struct|Enum, Trait)` arm returns the concrete
    // side — so a genuinely heterogeneous list built by `push` compiled and ran.
    // Refused at the argument, by the same rule and at the same arm.
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun main() { mut xs: List<A> = []; xs.push(Bag { n = 1 }); }
        "#,
        "'A' is a trait, not a type",
    );
}

// The three routes to B55's internal error (§2.1). The B4 amendment recorded
// one; the paper's P7 found three, byte-identical, and they are exactly the
// accepting positions of the table above meeting a bounded generic. Each used
// to abort the build from the TRANSFORMER with "please report this program",
// with the caret on the trait's own `fun show(self): str;` — inside std, for a
// std trait — while the message said "at this call". Each is an ordinary
// declaration error now, at the annotation the author wrote.

#[test]
fn b4_the_internal_error_route_through_a_binding_is_a_clean_refusal() {
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun use_it<T: A>(v: T): str { v.name() }
        fun main() { let x: A = Bag { n = 1 }; let s = use_it(x); }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b4_the_internal_error_route_through_a_field_is_a_clean_refusal() {
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        struct Holder { item: A }
        fun use_it<T: A>(v: T): str { v.name() }
        fun main() { let h = Holder { item = Bag { n = 1 } }; let s = use_it(h.item); }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b4_the_internal_error_route_through_a_return_is_a_clean_refusal() {
    assert_fails_with(
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun make(): A { Bag { n = 1 } }
        fun use_it<T: A>(v: T): str { v.name() }
        fun main() { let s = use_it(make()); }
        "#,
        "'A' is a trait, not a type",
    );
}

#[test]
fn b4_no_route_to_the_internal_error_survives() {
    // The half that makes the three above a closure rather than three fixes:
    // whatever else the programs report, none of them reaches the transformer's
    // B55 guard. "internal:" in a diagnostic is the string that must not
    // appear, because it is the one that asks the user to file a bug for their
    // own mistake.
    for source in [
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun use_it<T: A>(v: T): str { v.name() }
        fun main() { let x: A = Bag { n = 1 }; let s = use_it(x); }
        "#,
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        struct Holder { item: A }
        fun use_it<T: A>(v: T): str { v.name() }
        fun main() { let h = Holder { item = Bag { n = 1 } }; let s = use_it(h.item); }
        "#,
        r#"
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun make(): A { Bag { n = 1 } }
        fun use_it<T: A>(v: T): str { v.name() }
        fun main() { let s = use_it(make()); }
        "#,
    ] {
        let diagnostics = compile(source).expect_err("expected a compile error");
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("internal:")),
            "an internal error survived: {diagnostics:#?}"
        );
    }
}

// The positions where a trait is the LEGITIMATE spelling, pinned so the
// tightening cannot creep into them. Each is a place the trait names a bound or
// a namespace rather than a value's type, and each is load-bearing today.

#[test]
fn b4_a_trait_stays_legal_as_a_generic_bound() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun show<T: A>(v: T): str { v.name() }
        fun main() { print(show(Bag { n = 1 })); }
        main();
        "#,
        "bag\n",
    );
}

#[test]
fn b4_a_trait_stays_legal_as_a_supertrait() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait A { fun name(self): str; }
        trait B with A { fun louder(self): str { self.name() } }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        impl Bag with B { }
        fun main() { print(Bag { n = 1 }.louder()); }
        main();
        "#,
        "bag\n",
    );
}

#[test]
fn b4_a_trait_stays_legal_as_an_impl_subject() {
    // std's own `impl Iterator<type T> with Iterable<T>` shape — a blanket impl
    // over a bound, where the subject IS a trait. The `Iterable` surface has to
    // keep compiling exactly as it did.
    assert_compiles(
        r#"
        import std::option::Option::{ self, Some, None };
        trait Walk<T> { fun step(self): Option<T>; }
        trait AsWalk<T> { fun as_walk(self): Self; }
        impl Walk<type T> with AsWalk<T> { fun as_walk(self): Self { self } }
        fun main() {}
        "#,
    );
}

#[test]
fn b4_a_trait_stays_legal_as_a_qualified_call_head() {
    // B57's trait-qualified call and B83's trait-provided static both write the
    // trait's name at the head of a path. A path head selects a namespace; it
    // is not a value position.
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }
        fun main() { print(A::name(Bag { n = 1 })); }
        main();
        "#,
        "bag\n",
    );
}

#[test]
fn b4_a_self_defaulted_parameter_stays_legal() {
    // `trait Add<B = Self>` makes the NAME `B` resolve to the very same
    // `Type::Trait` that `Self` does — std's operator traits are all written
    // this way. The refusal keys on the entity the name resolves to, not on the
    // type it produces, so a generic parameter is never mistaken for the trait.
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait Combine<B = Self> { fun combine(self, other: B): str; }
        struct Bag { n: i32 }
        impl Bag with Combine { fun combine(self, other: Bag): str { "combined" } }
        fun main() { print(Bag { n = 1 }.combine(Bag { n = 2 })); }
        main();
        "#,
        "combined\n",
    );
}

// --- B98 (B4 §2.4): two impls of one trait for one subject ------------------
//
// `trait-objects.md` §2.4 (probe P11) found the gap between two shipped checks:
// B57 hard-errors on duplicate INHERENT members and on a trait-vs-trait
// ambiguity, and B74 closed duplicate statics, but the same trait implemented
// twice for the same type fell between them and resolved by DECLARATION ORDER.
// The second impl was emitted nowhere and was silently dead, at a direct call
// and through a bounded generic alike, and which one died depended on the order
// the modules happened to load in.
//
// Both of those checks exempt a trait-provided NAME on purpose, so that two
// impls of one trait stay legal — `method-resolution.md` §9(6)'s platform twins
// and the std `Into` blanket. B98 is the rule that owns the other side of that
// exemption: the members stay exempt, the impl PAIR does not.
//
// The pair key is `(trait, trait arguments, subject)`, compared for SAMENESS
// rather than compatibility: a generic position matches only another generic
// position bound the same way. That is what keeps the three carve-outs below
// working, each by the rule rather than by exemption.

#[test]
fn b98_duplicate_trait_impls_are_refused_at_both_call_paths() {
    // P11, exactly — the program whose two calls both used to print `first`.
    assert_fails_with(
        r#"
        import std::print;
        trait Show { fun show(self): str; }
        struct Bag { n: i32 }
        impl Bag with Show { fun show(self): str { "first" } }
        impl Bag with Show { fun show(self): str { "second" } }
        fun via<T: Show>(v: T): str { v.show() }
        fun main() { let b = Bag { n = 1 }; print(b.show()); print(via(b)); }
        main();
        "#,
        "is already implemented for 'Bag'",
    );
}

#[test]
fn b4_two_impls_of_one_trait_for_one_type_are_an_error() {
    // The end state §2.4 asked for, in B57/B74's register: a definition-site
    // error at the second impl, noting the first.
    assert_fails_with(
        r#"
        trait Show { fun show(self): str; }
        struct Bag { n: i32 }
        impl Bag with Show { fun show(self): str { "first" } }
        impl Bag with Show { fun show(self): str { "second" } }
        fun main() { let b = Bag { n = 1 }; }
        "#,
        "is already implemented for 'Bag'",
    );
}

#[test]
fn b98_a_duplicate_of_a_std_impl_names_the_module_it_came_from() {
    // The gap-b shape one family over (`method-resolution.md` §2.1): a user file
    // re-declaring an impl std already has. The report is B57's cross-file one —
    // the note renders std's own line, and the one-line form says which module.
    assert_fails_with(
        r#"
        import std::option::Option;
        import std::operators::Lift;
        impl Option<type T> with Lift {}
        fun main() { }
        "#,
        "is already implemented for 'Option<T>' by module 'option'",
    );
}

#[test]
fn b98_one_with_clause_naming_a_trait_twice_is_the_same_pair() {
    // `impl Bag with Show + Show` — a pair inside one clause. B84's block rule
    // cannot see it (it reads declared MEMBERS, and there is one `show`), and it
    // is a duplicate by exactly the same argument as two blocks.
    assert_fails_with(
        r#"
        trait Show { fun show(self): str; }
        struct Bag { n: i32 }
        impl Bag with Show + Show { fun show(self): str { "x" } }
        fun main() { }
        "#,
        "is already implemented for 'Bag'",
    );
}

#[test]
fn b98_a_trait_subject_gets_the_same_rule() {
    // The subject may itself BE a trait — std's blanket-over-a-bound shape
    // (`impl Iterator<type T> with Iterable<T>`), so the `Trait` arm of the
    // sameness walk carries real weight.
    assert_fails_with(
        r#"
        trait Walk { fun step(self): i32; }
        trait Marker { fun mark(self): i32; }
        impl Walk with Marker { fun mark(self): i32 { 1 } }
        impl Walk with Marker { fun mark(self): i32 { 2 } }
        fun main() { }
        "#,
        "is already implemented for 'Walk'",
    );
}

#[test]
fn b98_two_generic_impls_differing_only_in_binder_name_are_one_impl() {
    // `impl Pair<type T>` and `impl Pair<type U>` are the same impl written
    // twice, and the second is dead exactly as P11's is. The subject TYPE IDS
    // differ here even though the types are equal — types are not interned — so
    // the rule cannot be identity on the id.
    assert_fails_with(
        r#"
        trait Show { fun show(self): str; }
        struct Pair<T> { value: T }
        impl Pair<type T> with Show { fun show(self): str { "a" } }
        impl Pair<type U> with Show { fun show(self): str { "b" } }
        fun main() { }
        "#,
        "is already implemented for",
    );
}

#[test]
fn b98_an_elided_self_default_does_not_disguise_a_duplicate() {
    // `trait Combine<B = Self>`: the `with` clause records the arguments it
    // WROTE, so `with Combine` records none and `with Combine<Bag>` records one.
    // They are the same instantiation, and the rule compares the effective
    // arguments — the written ones padded with the trait's declared defaults,
    // `= Self` meaning the subject.
    assert_fails_with(
        r#"
        trait Combine<B = Self> { fun combine(self, other: B): str; }
        struct Bag { n: i32 }
        impl Bag with Combine { fun combine(self, other: Bag): str { "x" } }
        impl Bag with Combine<Bag> { fun combine(self, other: Bag): str { "y" } }
        fun main() { }
        "#,
        "is already implemented for 'Bag'",
    );
}

#[test]
fn b98_the_same_trait_at_different_arguments_is_two_impls() {
    // The reason trait ARGUMENTS are in the pair key: `Into<Bar>` and
    // `Into<Baz>` are two implementations, not one written twice, and refusing
    // them would make a parameterized trait implementable once per type.
    assert_compiles(
        r#"
        trait Tag<A> { fun tag(self): A; }
        struct Bag { n: i32 }
        struct Cup { n: i32 }
        impl Bag with Tag<Cup> { fun tag(self): Cup { Cup { n = 1 } } }
        impl Bag with Tag<Bag> { fun tag(self): Bag { Bag { n = 2 } } }
        fun main() { }
        "#,
    );
}

#[test]
fn b98_a_user_reflexive_into_impl_is_legal_and_not_a_duplicate() {
    // Originally the carve-out that would have taken std down: a rule built on
    // `compare_type` would have called std's `impl type T with Into<T>` a
    // duplicate of every user `Into` impl. That blanket is deleted (B127,
    // method-resolution.md §14), and this program — unchanged — is now the
    // migration path's legality pin: a USER-written reflexive impl
    // (`impl Fahrenheit with Into<Fahrenheit>`) is an ordinary impl, legal
    // beside a converting `Into<Fahrenheit>` on another subject, because the
    // pair key is `(trait, arguments, subject)` and the subjects differ.
    assert_compiles(
        r#"
        import std::into::Into;
        struct Celsius { degrees: i32 }
        struct Fahrenheit { degrees: i32 }
        impl Celsius with Into<Fahrenheit> {
            fun into(self): Fahrenheit { Fahrenheit { degrees = self.degrees * 2 } }
        }
        impl Fahrenheit with Into<Fahrenheit> {
            fun into(self): Fahrenheit { self }
        }
        fun main() { }
        "#,
    );
}

#[test]
fn b98_two_bounded_impls_differing_only_in_binder_name_are_one_impl() {
    // The other direction of the bounds rule, and the case that needs it: an
    // UNBOUNDED impl binder inherits the subject type's own constraint id (B77),
    // so two of them are already the same id, while a bound mints a fresh one.
    // `Pair<type T: Show>` and `Pair<type U: Show>` are therefore two different
    // ids for one position, and only comparing what they are BOUND by sees it.
    assert_fails_with(
        r#"
        trait Show { fun show(self): str; }
        trait Label { fun label(self): str; }
        struct Pair<T> { value: T }
        impl Pair<type T: Show> with Label { fun label(self): str { "a" } }
        impl Pair<type U: Show> with Label { fun label(self): str { "b" } }
        fun main() { }
        "#,
        "is already implemented for",
    );
}

#[test]
fn b98_two_conditional_impls_with_different_bounds_are_two_impls() {
    // B73 one level in: two generic positions are the same position only when
    // they carry the same BOUNDS. `Pair<T: Show>` and `Pair<U: Marker>` overlap
    // wherever a type is both, which is a specificity question and not a repeat.
    assert_compiles(
        r#"
        trait Show { fun show(self): str; }
        trait Marker { fun mark(self): i32; }
        trait Label { fun label(self): str; }
        struct Pair<T> { value: T }
        impl Pair<type T: Show> with Label { fun label(self): str { "shown" } }
        impl Pair<type U: Marker> with Label { fun label(self): str { "marked" } }
        fun main() { }
        "#,
    );
}

#[test]
fn b98_one_trait_for_two_subjects_and_two_traits_for_one_subject_stay_legal() {
    // The family's guard rail: the pair is (trait, subject), so neither half
    // repeating on its own is a duplicate.
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait Show { fun show(self): str; }
        trait Label { fun label(self): str; }
        struct Bag { n: i32 }
        struct Cup { n: i32 }
        impl Bag with Show { fun show(self): str { "bag" } }
        impl Cup with Show { fun show(self): str { "cup" } }
        impl Bag with Label { fun label(self): str { "labelled" } }
        fun main() { print(Bag { n = 1 }.show()); print(Cup { n = 2 }.show()); print(Bag { n = 3 }.label()); }
        main();
        "#,
        "bag\ncup\nlabelled\n",
    );
}

#[test]
fn b98_a_third_impl_is_reported_against_the_first() {
    // B57's counting rule: each later impl is reported against the FIRST it
    // repeats, so three copies produce two errors rather than one or three.
    let errors = compile(
        r#"
        trait Show { fun show(self): str; }
        struct Bag { n: i32 }
        impl Bag with Show { fun show(self): str { "a" } }
        impl Bag with Show { fun show(self): str { "b" } }
        impl Bag with Show { fun show(self): str { "c" } }
        fun main() { }
        "#,
    )
    .expect_err("expected the duplicate impls to be refused");
    assert_eq!(
        errors
            .iter()
            .filter(|error| error.contains("is already implemented for 'Bag'"))
            .count(),
        2,
        "{errors:#?}"
    );
}

#[test]
fn b98_the_platform_twins_are_not_a_duplicate_on_either_leg() {
    // The carve-out the rule must not break, pinned in BOTH directions:
    // `browser/ui.vl` and `process/ui.vl` each write `impl View with Slot`,
    // `impl str with Slot`, `impl Signal<str> with AttrValue` and three more —
    // the twin MECHANISM (`method-resolution.md` §9(6)). Module resolution is
    // layered, so exactly one `ui` loads per build and the pairs never meet;
    // nothing special-cases them, and this is the measurement that says so.
    // (std's own impls are frozen in a scoped run, so the full-scan half of the
    // proof lives in `check_scope_differential.rs`, which force-loads every std
    // module on both legs with the skip disabled.)
    let source = r#"
        import std::ui::{ View, view };
        fun main() { let root: View = view("div"); }
        "#;
    assert_compiles_browser(source);
    assert_compiles(source);
}

// --- B4 §2.2: a bare trait annotation must not launder a resource -----------
//
// `proposal/trait-objects.md` §2.2 (probes P8/P9): the resource analysis
// classifies `Type::Trait` as "never a resource by containment", and marks that
// verdict COMPLETE — correct for the `Self` meaning of `Type::Trait`, a lie for
// the "a user annotated a value with a trait" meaning. So annotating a
// resource-carrying value with a bare trait type silently deletes its
// destructor call and silently licenses a second owner: containment inference
// (`spec/memory.md` §341-346) cannot see through the trait type, so R1, R2 and
// R10 never fire and scope-end destruction never happens. This is `spec/
// memory.md` R12's laundering hazard through a sink R12 does not name, and
// unlike `any` it does not even produce a diagnostic. Live data loss, in
// shipped code, with no trait objects anywhere.
//
// Each leak case is paired with its CONCRETE control, which passes today and
// must keep passing: the controls are what make the pins non-vacuous — they
// prove the destructor machinery is live and that only the trait annotation
// suppresses it.
//
// The fix is the §12.2 tightening (a trait in value position is an error at
// the annotation), so the closed shape of each leak is a REFUSAL, not a
// destructor that runs: the resource never gets behind the trait in the first
// place. Closed by construction rather than by a check per position — which is
// the argument §2.2 makes for the tightening over any narrower patch.

#[test]
fn a_resource_binding_runs_its_destructor() {
    // P8's control arm, concrete annotation: `drop` runs at scope end.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Handle { id: i32 }
        impl Handle with Drop { fun drop(&mut self): void { print("closing"); } }
        trait Named { fun name(self): str; }
        impl Handle with Named { fun name(self): str { "h" } }
        fun main() {
            let handle: Handle = Handle { id = 1 };
            print("ok");
        }
        main();
        "#,
        "ok\nclosing\n",
    );
}

#[test]
fn a_bare_trait_binding_cannot_swallow_a_resources_destructor() {
    // P8 row 2. Today this compiles, runs, prints `ok` and NEVER prints
    // `closing` — one changed word in the annotation deleted the destructor.
    assert_fails_with(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Handle { id: i32 }
        impl Handle with Drop { fun drop(&mut self): void { print("closing"); } }
        trait Named { fun name(self): str; }
        impl Handle with Named { fun name(self): str { "h" } }
        fun main() {
            let handle: Named = Handle { id = 1 };
            print("ok");
        }
        main();
        "#,
        "a trait is not a value type (vilan has no trait objects)",
    );
}

#[test]
fn a_resource_field_runs_its_destructor() {
    // P8's control arm, by containment: an aggregate holding a resource behind
    // a CONCRETE field is a resource, and its scope end destroys it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Handle { id: i32 }
        impl Handle with Drop { fun drop(&mut self): void { print("closing"); } }
        trait Named { fun name(self): str; }
        impl Handle with Named { fun name(self): str { "h" } }
        struct Holder { item: Handle }
        fun main() {
            let holder = Holder { item = Handle { id = 1 } };
            print("ok");
        }
        main();
        "#,
        "ok\nclosing\n",
    );
}

#[test]
fn a_bare_trait_field_cannot_swallow_a_resources_destructor() {
    // P8 row 4 — the field route, which is the dangerous one: the resource is
    // reachable, owned, and invisible to containment inference.
    assert_fails_with(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Handle { id: i32 }
        impl Handle with Drop { fun drop(&mut self): void { print("closing"); } }
        trait Named { fun name(self): str; }
        impl Handle with Named { fun name(self): str { "h" } }
        struct Holder { item: Named }
        fun main() {
            let holder = Holder { item = Handle { id = 1 } };
            print("ok");
        }
        main();
        "#,
        "a trait is not a value type (vilan has no trait objects)",
    );
}

#[test]
fn a_resource_field_keeps_its_single_owner() {
    // P9's control arm: through a concrete field, the affine checker sees the
    // resource and refuses the second owner.
    assert_fails_with(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Handle { id: i32 }
        impl Handle with Drop { fun drop(&mut self): void { print("closing"); } }
        trait Named { fun name(self): str; }
        impl Handle with Named { fun name(self): str { "h" } }
        struct Holder { item: Handle }
        fun main() {
            let holder = Holder { item = Handle { id = 1 } };
            let first = holder;
            let second = holder;
            print("ok");
        }
        main();
        "#,
        "use of `holder` after it was moved",
    );
}

#[test]
fn a_bare_trait_field_cannot_launder_the_single_owner_rule() {
    // P9. Today this compiles, runs, prints `ok`, and emits
    // `const holder = [ [ 1 ] ];` — one resource, two live owners, no drop.
    assert_fails(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Handle { id: i32 }
        impl Handle with Drop { fun drop(&mut self): void { print("closing"); } }
        trait Named { fun name(self): str; }
        impl Handle with Named { fun name(self): str { "h" } }
        struct Holder { item: Named }
        fun main() {
            let holder = Holder { item = Handle { id = 1 } };
            let first = holder;
            let second = holder;
            print("ok");
        }
        main();
        "#,
    );
}

// --- B74: the duplicate check reaches statics (method-resolution.md §9) -----
//
// B57's duplicate-inherent check filtered `is_self_method`, per its own scope,
// so two impls declaring the same `fun new()` for one subject stayed a silent
// pick — the same dead-declaration hazard one receiver position away. An impl's
// `declarations` is ONE map keyed by name, so receiver position was never part
// of a member's identity; the filter was doing double duty as "methods only"
// and "same namespace only", and only the first was meant. The sweep that ran
// before this landed found ZERO live collisions across std, the corpus, every
// example and every compiled docs fence — the entire blast radius is the shapes
// pinned here.

#[test]
fn b74_two_static_declarations_of_one_name_are_an_error() {
    // The filed shape. Before this, `Bag::new()` silently took the first.
    assert_fails_spanning_nth(
        r#"
        struct Bag { n: i32 }
        impl Bag { fun new(): Bag { Bag { n = 1 } } }
        impl Bag { fun new(): Bag { Bag { n = 2 } } }
        fun main() { let bag = Bag::new(); }
        "#,
        "new",
        1,
        "is already defined for 'Bag'",
    );
}

#[test]
fn b74_the_duplicate_static_error_notes_the_first_declaration() {
    // The same cross-file-capable note B57 ships for methods (§9(4)): the
    // second declaration is the anchor, the first gets the note.
    assert_fails_noting(
        r#"
        struct Bag { n: i32 }
        impl Bag { fun spawn_here(): Bag { Bag { n = 1 } } }
        impl Bag { fun spawn_here(): Bag { Bag { n = 2 } } }
        fun main() { let bag = Bag::spawn_here(); }
        "#,
        "is already defined for 'Bag'",
        "spawn_here",
        "is already defined here",
    );
}

#[test]
fn b74_a_static_and_a_method_of_one_name_collide() {
    // The truth about namespaces, pinned: there is only ONE. `declarations` is
    // keyed by name alone, and the static is the declaration that dies —
    // `Bag::tag()` resolves the inherent METHOD first and then fails on arity
    // ("`tag` expects 1 argument, but got 0 instead"), a report of a
    // declaration that was never reachable by either call form. So they
    // collide, and the error lands where the fix does.
    assert_fails_spanning_nth(
        r#"
        struct Bag { n: i32 }
        impl Bag { fun tag(): str { "static" } }
        impl Bag { fun tag(self): str { "method" } }
        fun main() { let bag = Bag { n = 1 }; }
        "#,
        "tag",
        1,
        "is already defined for 'Bag'",
    );
}

#[test]
fn b74_an_extra_static_in_a_trait_impl_block_counts_as_inherent() {
    // §9(2): "inherent" is a property of the MEMBER. A static written inside a
    // `with`-clause block that the trait does not declare is the type's own,
    // and collides with a plain inherent block's same name — the static twin of
    // `b57_an_extra_method_in_a_trait_impl_block_counts_as_inherent`.
    assert_fails_spanning_nth(
        r#"
        struct Bag { n: i32 }
        trait Marker { fun mark(self): str; }
        impl Bag with Marker {
            fun mark(self): str { "m" }
            fun make(): Bag { Bag { n = 1 } }
        }
        impl Bag { fun make(): Bag { Bag { n = 2 } } }
        fun main() { let bag = Bag::make(); }
        "#,
        "make",
        1,
        "is already defined for 'Bag'",
    );
}

#[test]
fn b74_a_trait_provided_static_does_not_collide_with_an_inherent_one() {
    // The load-bearing negative, and the reason the widening is a filter change
    // rather than a filter deletion. A trait's STATIC is homed by its trait, so
    // it never enters the inherent tier: one inherent declaration is left, and
    // one does not collide. `member_home_trait` reads the trait's declaration
    // map directly with no receiver filter, which is what makes it home a
    // static as readily as a method.
    //
    // Proven load-bearing against the tree, not by argument: with the homing
    // guard removed, `vilan/std/src/time.vl`'s inherent `Duration::describe`
    // collides with the `Wire` trait's `describe` and the corpus goes red.
    //
    // The note this pin carried — that `Bag::default()` here resolved to the
    // TRAIT's static (7) rather than the inherent one, because the static
    // accessor path never got B57's tiering — is now B83, fixed: it resolves
    // to the inherent `1`, and the value is pinned so the two facts stay
    // together. The trait's declaration is still not a DUPLICATE of the
    // inherent one, which is what B74 claims; it is outranked by it, which is
    // what B57 claims. Both at once, on one program.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::default::Default;

        struct Bag { n: i32 }

        impl Bag with Default {
            fun default(): Bag { Bag { n = 7 } }
        }

        impl Bag {
            fun default(): Bag { Bag { n = 1 } }
        }

        fun main() {
            print(Bag::default().n);
        }
        "#,
        "1\n",
    );
}

#[test]
fn b74_the_same_static_name_on_two_types_is_not_a_duplicate() {
    // Subject compatibility still gates the pair: `new` on two distinct types
    // is two declarations, not a duplicate.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { n: i32 }
        struct Box { n: i32 }
        impl Bag { fun new(): Bag { Bag { n = 1 } } }
        impl Box { fun new(): Box { Box { n = 2 } } }
        fun main() { print(Bag::new().n); print(Box::new().n); }
        main();
        "#,
        "1\n2\n",
    );
}

#[test]
fn b74_a_duplicate_static_on_a_generic_subject_is_an_error() {
    // Subjects are compared with `compare_type`, so an impl with `type` binders
    // collides with its twin exactly as a concrete one does.
    assert_fails_spanning_nth(
        r#"
        struct Cell<T> { value: T }
        impl Cell<type T> { fun of(value: T): Cell<T> { Cell { value = value } } }
        impl Cell<type T> { fun of(value: T): Cell<T> { Cell { value = value } } }
        fun main() { let cell = Cell::of(1); }
        "#,
        "of",
        1,
        "is already defined for",
    );
}

#[test]
fn b74_three_static_declarations_produce_two_errors() {
    // Each later declaration reports against the FIRST compatible one before
    // it, so the count follows the declarations, not the pairs.
    let source = r#"
        struct Bag { n: i32 }
        impl Bag { fun new(): Bag { Bag { n = 1 } } }
        impl Bag { fun new(): Bag { Bag { n = 2 } } }
        impl Bag { fun new(): Bag { Bag { n = 3 } } }
        fun main() { let bag = Bag::new(); }
        "#;
    match compile(source) {
        Ok(_) => panic!("expected two duplicate-static errors, but it compiled"),
        Err(errors) => {
            let duplicates = errors
                .iter()
                .filter(|error| error.contains("is already defined for 'Bag'"))
                .count();
            assert_eq!(duplicates, 2, "expected two duplicates; got: {errors:#?}");
        }
    }
}

#[test]
fn b74_a_static_still_resolves_when_it_is_the_only_one() {
    // The check must not disturb the ordinary path: one static, one home.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Bag { n: i32 }
        impl Bag {
            fun new(n: i32): Bag { Bag { n = n } }
            fun tag(self): str { "bag" }
        }
        fun main() { let bag = Bag::new(4); print(bag.n); print(bag.tag()); }
        main();
        "#,
        "4\nbag\n",
    );
}
