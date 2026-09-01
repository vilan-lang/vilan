//! Generic binding and monomorphization: `R8`, `[must_use]`, `[deprecated]`,
//! the unification seam, method and argument passing, and `Signal::update`.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- R8: no implicit borrow at the call site -------------------------------

#[test]
fn r8_explicit_borrow_and_reborrow() {
    // A `&`/`&mut` parameter takes an explicit `&[mut] place`, or an existing
    // view forwarded (re-borrowed) — both compile.
    assert_compiles(
        r#"
        fun bump(x: &mut i32) { x += 1; }
        fun via(y: &mut i32) { bump(y); }
        fun main() { mut a = 0; bump(&mut a); via(&mut a); }
        "#,
    );
}

#[test]
fn r8_method_receiver_is_implicitly_borrowed() {
    // R8 exempts the `self` receiver: `c.inc()` on a `&mut self` method needs no
    // `&mut c` at the call site.
    assert_compiles(
        r#"
        struct C { v: i32 }
        impl C { fun inc(&mut self) { self.v = self.v + 1; } }
        fun main() { mut c = C { v = 0 }; c.inc(); }
        "#,
    );
}

#[test]
fn r8_reject_implicit_borrow() {
    // Passing a bare value place to a `&mut` parameter is rejected — there is no
    // implicit borrow (a scalar would otherwise emit a broken `(base,key)` read).
    assert_fails(
        r#"
        fun bump(x: &mut i32) { x += 1; }
        fun main() { mut a = 0; bump(a); }
        "#,
    );
}

// --- [must_use] -------------------------------------------------------------

#[test]
fn must_use_dropped_result_warns() {
    // A dropped `[must_use]` result (a bare statement) is a warning.
    let messages = warnings(
        r#"
        [must_use]
        fun make(): i32 { 42 }
        fun main() { make(); }
        "#,
    );
    assert!(
        messages.iter().any(|message| message.contains("must_use")),
        "expected a must_use warning, got {messages:?}"
    );
}

#[test]
fn must_use_consumed_result_no_warning() {
    // Binding, discarding with `let _`, or passing as an argument consumes the
    // result — no warning.
    let messages = warnings(
        r#"
        import std::io::print;
        [must_use]
        fun make(): i32 { 42 }
        fun consume(x: i32) { print(x); }
        fun main() {
            let a = make();
            let _ = make();
            consume(make());
            print(a);
        }
        "#,
    );
    assert!(
        messages.is_empty(),
        "expected no warnings, got {messages:?}"
    );
}

// --- [deprecated] (proposal/deprecation.md) ---------------------------------

/// A copy of the real std with one extra module, `std::deprecated_probe`,
/// whose `stale()` is `[deprecated("use fresh()")]` and whose `wrapper()`
/// calls it std-internally — the §5 mechanism leg's std half, end to end
/// through `resolve_std` and the module loader (the copy's modules register
/// as `std_sources` exactly as the real std's do).
fn deprecation_fixture_std(tag: &str) -> (PathBuf, PackageSpec) {
    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("create the fixture std directory");
        for entry in std::fs::read_dir(from).expect("read the std tree") {
            let entry = entry.expect("read a std tree entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("stat a std tree entry").is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).expect("copy a std source");
            }
        }
    }
    let root =
        std::env::temp_dir().join(format!("vilan-deprecated-std-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // `macro_std` rides along: the macro world resolves it BESIDE `std`.
    let tree = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan");
    let std_root = root.join("std");
    copy_tree(&tree.join("std"), &std_root);
    copy_tree(&tree.join("macro_std"), &root.join("macro_std"));
    std::fs::write(
        std_root.join("src").join("deprecated_probe.vl"),
        r#"// Test-only module: a deprecated item beside its replacement, plus a
// std-internal caller (deprecation.md §5's mechanism leg).
fun fresh(): i32 { 1 }

[deprecated("use fresh()")]
fun stale(): i32 { 1 }

fun wrapper(): i32 { stale() }
"#,
    )
    .expect("write the fixture std module");
    let spec = vilan_core::manifest::resolve_std(&std_root);
    (root, spec)
}

#[test]
fn a_deprecated_functions_call_warns_at_the_callee_name() {
    // The family head, verbatim, anchored at the callee NAME (A1/A4 — names
    // over argument lists). The use precedes the declaration so the first
    // occurrence of `one` is the use site.
    assert_warns_spanning(
        r#"
        fun main() { one(); }
        [deprecated("use two()")]
        fun one() { }
        fun two() { }
        "#,
        "one",
        "`one` is deprecated; use two()",
    );
}

#[test]
fn an_unmarked_function_does_not_warn() {
    let messages = warnings(
        r#"
        fun one() { }
        fun main() { one(); }
        "#,
    );
    assert!(
        messages.is_empty(),
        "expected no warnings, got {messages:?}"
    );
}

#[test]
fn every_use_site_of_a_deprecated_function_warns_independently() {
    // Per USE SITE, not once per form (§1, B5): two calls and a passed-as-value
    // reference are three independent fixes, so three warnings.
    let warnings = warning_diagnostics(
        r#"
        fun apply(action: sync || void) { action(); }
        fun main() { old(); old(); apply(old); }
        [deprecated("use fresh()")]
        fun old() { }
        fun fresh() { }
        "#,
    );
    let deprecation_count = warnings
        .iter()
        .filter(|(message, _)| message.contains("`old` is deprecated; use fresh()"))
        .count();
    assert_eq!(
        deprecation_count, 3,
        "each use site warns once; got {warnings:#?}"
    );
}

#[test]
fn a_use_inside_another_deprecated_item_still_warns() {
    // DECIDED (recorded in deprecation.md's ship record): no Rust-style
    // suppression inside deprecated items. Per §1 each use site is an
    // independent fix — the deprecated wrapper's body must migrate too, and
    // std's own hygiene rule (migrate your callers in the deprecating train)
    // reads the same way for user code.
    let warnings = warning_diagnostics(
        r#"
        fun main() { old_outer(); }
        [deprecated("use fresh()")]
        fun old_outer() { old_inner(); }
        [deprecated("use fresh()")]
        fun old_inner() { }
        fun fresh() { }
        "#,
    );
    let heads: Vec<&str> = warnings
        .iter()
        .filter(|(message, _)| message.contains("is deprecated"))
        .map(|(message, _)| message.as_str())
        .collect();
    assert_eq!(
        heads,
        vec![
            "`old_outer` is deprecated; use fresh()",
            "`old_inner` is deprecated; use fresh()",
        ],
        "both use sites warn — the one in `main` and the one inside the deprecated wrapper"
    );
}

#[test]
fn a_deprecated_method_warns_at_the_member_name() {
    // The attribute parses on a member `fun` (one `parse_function`), and the
    // warning anchors at the MEMBER name (`.old_area`), not the whole access.
    assert_warns_spanning(
        r#"
        fun main() {
            let sq = Square { side = 3 };
            let _ = sq.old_area();
        }
        struct Square { side: i32 }
        impl Square {
            [deprecated("use area()")]
            fun old_area(self): i32 { self.side * self.side }
            fun area(self): i32 { self.side * self.side }
        }
        "#,
        "old_area",
        "`old_area` is deprecated; use area()",
    );
}

#[test]
fn a_std_marked_item_warns_at_its_use() {
    // The std half of §5's mechanism leg: an item marked in (fixture) std
    // source warns at its user-code use, head and span, through the real
    // module loader.
    let (root, std) = deprecation_fixture_std("warns");
    let source = r#"
        import std::deprecated_probe::{ stale };
        fun main() { let _ = stale(); }
    "#;
    let warnings = warning_diagnostics_with_std(source, std);
    let matching: Vec<_> = warnings
        .iter()
        .filter(|(message, _)| message.contains("`stale` is deprecated; use fresh()"))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "exactly the call site warns (the import line is not a use); got {warnings:#?}"
    );
    let expected = source.rfind("stale").expect("the call site");
    assert_eq!(
        matching[0].1,
        expected..expected + "stale".len(),
        "the warning anchors at the callee name"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_deprecated_form_used_only_inside_std_stays_silent() {
    // A2: the check keys on the use site's source. `wrapper()` calls the
    // deprecated `stale()` INSIDE std — the user program that only calls
    // `wrapper` sees no warning, which is also what keeps the
    // std-must-be-warning-clean gate green.
    let (root, std) = deprecation_fixture_std("silent");
    let warnings = warning_diagnostics_with_std(
        r#"
        import std::deprecated_probe::{ wrapper };
        fun main() { let _ = wrapper(); }
        "#,
        std,
    );
    assert!(
        warnings.is_empty(),
        "a std-internal use of a deprecated form is silent; got {warnings:#?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn enum_constructor_propagates_expected_type_to_payload() {
    // Bidirectional inference (B1): a constructor argument is typed against the
    // *expected* enum's arguments, not the abstract parameter. `Ok(Option::from_json
    // (t))` in a `Result<Option<User>, str>` context types `from_json` against
    // `Option<User>`, so it round-trips. (Was a generic-binding-flow bug.)
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        [derive(Json)] struct User { id: i32, name: str }
        fun main() {
            let decoded: Result<Option<User>, str> =
                Option::from_json("{\"id\":1,\"name\":\"Ada\"}");
            match decoded {
                Ok(Some(let u)) => print(u.name),
                Ok(None) => print("none"),
                Err(let e) => print(e),
            }
        }
        "#,
        "Ada\n",
    );
}

// --- Known bugs: generic-binding flow (backlog B1, see proposal/type-solver.md) ---
//
// These assert the *desired* behaviour and are `#[ignore]`d because they currently
// produce `undefined` — the two remaining faces of the generic-binding-flow class.
// Remove `#[ignore]` as each lands.

#[test]
fn generic_field_method_dispatch_runs() {
    // `(self.inner).handle(x)` on a generic-bounded field. Field access now
    // substitutes the struct's declared field generic through the subject's actual
    // arguments (`resolve_field_accessor`), so `self.inner` carries the receiver's
    // `T` id rather than the struct definition's — the dispatch binding composes
    // through `current_substitution` and emits the concrete `Doubler::handle`
    // instead of the empty abstract trait method.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Handler { fun handle(self, x: i32): i32; }
        struct Doubler { factor: i32 }
        impl Doubler with Handler { fun handle(self, x: i32): i32 { x * self.factor } }
        struct Wrap<T: Handler> { inner: T }
        impl Wrap<type T: Handler> {
            fun run(self, x: i32): i32 { (self.inner).handle(x) }
        }
        fun main() { let w = Wrap { inner = Doubler { factor = 3 } }; print(w.run(7)); }
        "#,
        "21\n",
    );
}

#[test]
fn generic_field_from_a_variable_dispatches() {
    // Same as above but the field value is a *variable*, so the `Wrap` initializer
    // (priority 1) is reached before `d` is grounded (priority 10) and defers. It
    // must not publish a type while deferred (the unbound parameter would fall back
    // to its constraint, `Wrap<Handler>`), and a pending generic initializer infers
    // as `Unresolved` so `let w = ..` defers instead of grounding on an abstract
    // `Wrap`. With both, `w` grounds to `Wrap<Doubler>` once the initializer
    // resolves, and the dispatch reaches the concrete `Doubler::handle`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Handler { fun handle(self, x: i32): i32; }
        struct Doubler { factor: i32 }
        impl Doubler with Handler { fun handle(self, x: i32): i32 { x * self.factor } }
        struct Wrap<T: Handler> { inner: T }
        impl Wrap<type T: Handler> {
            fun run(self, x: i32): i32 { (self.inner).handle(x) }
        }
        fun main() {
            let d = Doubler { factor = 3 };
            let w = Wrap { inner = d };
            print(w.run(7));
        }
        "#,
        "21\n",
    );
}

#[test]
fn from_json_indirect_element_type_runs() {
    // `decode` returns `Result<Option<User>, str>`; its body is now inferred against
    // that return type (the `ReturnType` constraint), so `Ok(Option::from_json(text))`
    // types `from_json` against `Option<User>` — the constructor propagation (fix #1)
    // then threads `User` into the decode. Previously the body was inferred bottom-up
    // and lowered to the abstract `from_json_value` → `Some(undefined)`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        [derive(Json)] struct User { id: i32, name: str }
        fun decode(text: str): Result<Option<User>, str> { Option::from_json(text) }
        fun main() {
            match decode("{\"id\":1,\"name\":\"Ada\"}") {
                Ok(Some(let u)) => print(u.name),
                Ok(None) => print("none"),
                Err(let e) => print(e),
            }
        }
        "#,
        "Ada\n",
    );
}

#[test]
fn deep_dependency_chain_resolves_across_passes() {
    // Ordering test for the dependency-driven re-queue (item 5 v2): each `id` call's
    // generic `T` binds from its argument, which is the *next* `id` call — so the
    // outer calls can only resolve several passes after the innermost. The runner
    // wakes each deferred call when its input lands (with the run-all backstop as a
    // safety net), so the whole nest resolves to `i32` and prints `7`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        fun id<T>(x: T): T { x }
        fun main() {
            let deep = id(id(id(id(id(id(7))))));
            print(format(deep));
        }
        "#,
        "7\n",
    );
}

#[test]
fn from_json_return_type_flows_through_match_arm() {
    // The RPC-client shape: the `from_json` decode sits inside a `match` arm whose
    // enclosing function declares the return type. The return type must reach the
    // arm body *through* the match — `resolve_match` propagates the function's
    // expected type into each leg, so `Ok(Option::from_json(json))` binds `User`
    // even though a `match` sits between the call and the signature. Without the
    // propagation the leg was inferred bottom-up → abstract decoder → `Some(undefined)`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        [derive(Json)] struct User { id: i32, name: str }
        fun decode(tag: str, json: str): Result<Option<User>, str> {
            match tag {
                "ok" => Option::from_json(json),
                _ => Err("bad tag"),
            }
        }
        fun main() {
            match decode("ok", "{\"id\":1,\"name\":\"Ada\"}") {
                Ok(Some(let u)) => print(u.name),
                Ok(None) => print("none"),
                Err(let e) => print(e),
            }
        }
        "#,
        "Ada\n",
    );
}

// --- Monomorphization unification (the one `emit_instance` / `call_substitution`
//     path; commit 6b96d3f) and dependency re-queue (item 5 v2) edge cases --------

#[test]
fn multi_parameter_generic_function_instantiations() {
    // The unified emitter keys an instance by its bound types ordered by constraint
    // id; the old free-function emitter keyed by *positional* type arguments. For a
    // two-parameter function those orders coincide (constraint ids are minted in
    // parameter order), and this pins that: `first<A, B>` must instantiate
    // `<i32, str>`, the *swapped* `<str, i32>`, and the same-type `<i32, i32>` as
    // distinct, non-colliding instances — a key bug would cross-wire them.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun first<A, B>(a: A, b: B): A { a }
        fun second<A, B>(a: A, b: B): B { b }
        fun main() {
            print(first(1, "x"));
            print(first("y", 2));
            print(second(1, "z"));
            print(first(3, 4));
        }
        "#,
        "1\ny\nz\n3\n",
    );
}

#[test]
fn multi_parameter_generic_method_monomorphizes() {
    // A two-generic impl whose methods return each parameter — the binding flows
    // through `method_call_substitution` (both `A` and `B` bound from the receiver
    // `Pair<i32, str>`) and field access substitutes the field's declared generic
    // through the receiver's arguments. Both reach the one `emit_instance` path.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct Pair<A, B> { left: A, right: B }
        impl Pair<type A, type B> {
            fun show_left(self): A { self.left }
            fun show_right(self): B { self.right }
        }
        fun main() {
            let p = Pair { left = 7, right = "hi" };
            print(p.show_left());
            print(p.show_right());
        }
        "#,
        "7\nhi\n",
    );
}

#[test]
fn operator_monomorphizes_on_generic_aggregate() {
    // `==` on `Option<Point>` overloads to the aggregate's `eq`, monomorphized
    // against the recorded type-arg substitution — the operator path through
    // `binary_op_dispatch` + `method_call_substitution` into the one emitter.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        [derive(PartialEq)] struct Point { x: i32, y: i32 }
        fun main() {
            let a: Option<Point> = Some(Point { x = 1, y = 2 });
            let b: Option<Point> = Some(Point { x = 1, y = 2 });
            let c: Option<Point> = Some(Point { x = 9, y = 9 });
            if a == b { print("ab-eq") } else { print("ab-neq") }
            if a == c { print("ac-eq") } else { print("ac-neq") }
        }
        "#,
        "ab-eq\nac-neq\n",
    );
}

#[test]
fn b135_operator_at_all_native_binding_monomorphizes_an_explicit_eq_body() {
    // B135, the OPERATOR half of B127's family: a conditional impl whose body
    // calls the trait method EXPLICITLY, invoked through `==`/`!=` at an
    // all-native binding. The operator path used to skip monomorphization
    // for all-native bindings (assuming the body uses only operators on `T`,
    // which lower native), so the explicit `.eq()` fell through to
    // `PartialEq`'s bodyless requirement and tripped the emitter's
    // never-silent check. The body REQUIRES the substitution, so the
    // emission specializes.
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::io::print;
        struct Pair<T> { a: T, b: T }
        impl Pair<type T: PartialEq> with PartialEq {
            fun eq(self, b: Pair<T>): bool {
                self.a.eq(b.a) && self.b.eq(b.b)
            }
        }
        fun main() {
            let x = Pair { a = 1, b = 2 };
            let y = Pair { a = 1, b = 2 };
            let z = Pair { a = 9, b = 9 };
            if x == y { print("xy-eq") } else { print("xy-neq") }
            if x == z { print("xz-eq") } else { print("xz-neq") }
            if x != z { print("xz-ne") } else { print("xz-not-ne") }
        }
        "#,
        "xy-eq\nxz-neq\nxz-ne\n",
    );
}

#[test]
fn b135_operator_reaches_the_requirement_through_a_generic_helper() {
    // The transitive shape: the operator body itself calls no requirement —
    // a generic HELPER it calls does. The specialize-or-not decision walks
    // the call graph, so the helper's `.eq()` still forces the instance.
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::io::print;
        struct Pair<T> { a: T, b: T }
        fun both_equal<E: PartialEq>(p: E, q: E, r: E, s: E): bool {
            p.eq(q) && r.eq(s)
        }
        impl Pair<type T: PartialEq> with PartialEq {
            fun eq(self, b: Pair<T>): bool {
                both_equal(self.a, b.a, self.b, b.b)
            }
        }
        fun main() {
            let x = Pair { a = 1, b = 2 };
            let y = Pair { a = 1, b = 2 };
            let z = Pair { a = 9, b = 9 };
            if x == y { print("xy-eq") } else { print("xy-neq") }
            if x == z { print("xz-eq") } else { print("xz-neq") }
        }
        "#,
        "xy-eq\nxz-neq\n",
    );
}

#[test]
fn b135_operator_reaches_the_requirement_through_a_closure() {
    // The closure shape: the requirement call hides inside a closure the
    // body creates and calls through a variable — an `Indirect` edge in the
    // call graph. The decision walks a node's LEXICAL closures too, so the
    // hidden `.eq()` still forces the instance.
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::io::print;
        struct Pair<T> { a: T, b: T }
        impl Pair<type T: PartialEq> with PartialEq {
            fun eq(self, b: Pair<T>): bool {
                let check = |x: T, y: T| x.eq(y);
                check(self.a, b.a) && check(self.b, b.b)
            }
        }
        fun main() {
            let x = Pair { a = 1, b = 2 };
            let y = Pair { a = 1, b = 2 };
            let z = Pair { a = 9, b = 9 };
            if x == y { print("xy-eq") } else { print("xy-neq") }
            if x == z { print("xz-eq") } else { print("xz-neq") }
        }
        "#,
        "xy-eq\nxz-neq\n",
    );
}

#[test]
fn single_level_container_from_json_roundtrip_runs() {
    // A single-level `List<i32>` decode: `from_json` calls `from_json_value`, whose
    // element type comes only from the enclosing `List<i32>` instantiation — the
    // inherited-substitution channel of `call_substitution`. Verifies it threads the
    // element type at runtime (the nested case is still open — see the ignored test).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::result::Result::{ self, Ok, Err };
        fun main() {
            let nums: Result<List<i32>, str> = List::from_json("[1,2,3]");
            match nums {
                Ok(let ns) => print(ns.to_json()),
                Err(let e) => print(e),
            }
        }
        "#,
        "[1,2,3]\n",
    );
}

#[test]
fn nested_container_from_json_roundtrip_runs() {
    // The `List<List<T>>` round-trip (the last row of the type-solver bug table).
    // The inner `List`'s element binding must thread through the *outer*
    // `from_json_value`: `resolve_dispatch` now binds an impl's generics from the
    // concrete receiver type (`bind_generics`) and emits a monomorphized instance,
    // so the nested `T::from_json_value` resolves at each level instead of lowering
    // to the abstract decoder (which yielded `[[undefined,...]]`). Triple nesting
    // exercises the recursion through two intermediate container instances.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::result::Result::{ self, Ok, Err };
        fun main() {
            let grid: Result<List<List<i32>>, str> = List::from_json("[[1,2],[3,4]]");
            match grid {
                Ok(let g) => print(g.to_json()),
                Err(let e) => print(e),
            }
            let deep: Result<List<List<List<i32>>>, str> = List::from_json("[[[1]],[[2,3]]]");
            match deep {
                Ok(let d) => print(d.to_json()),
                Err(let e) => print(e),
            }
        }
        "#,
        "[[1,2],[3,4]]\n[[[1]],[[2,3]]]\n",
    );
}

#[test]
fn mixed_nested_container_from_json_roundtrips() {
    // Mixed nesting through the same monomorphizing dispatch: `Option<List<i32>>`,
    // `List<Option<i32>>` (with a JSON `null` -> `None`), and a `List` of derived
    // structs — each inner decoder is monomorphized for its element via the impl's
    // generics bound from the concrete type.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        [derive(Json)] struct P { x: i32 }
        fun main() {
            let a: Result<Option<List<i32>>, str> = Option::from_json("[1,2,3]");
            match a {
                Ok(let av) => print(av.to_json()),
                Err(let e) => print(e),
            }
            let b: Result<List<Option<i32>>, str> = List::from_json("[1,null,3]");
            match b {
                Ok(let bv) => print(bv.to_json()),
                Err(let e) => print(e),
            }
            let c: Result<List<P>, str> = List::from_json("[{\"x\":1},{\"x\":2}]");
            match c {
                Ok(let cv) => print(cv.to_json()),
                Err(let e) => print(e),
            }
        }
        "#,
        "[1,2,3]\n[1,null,3]\n[{\"x\":1},{\"x\":2}]\n",
    );
}

// --- Method & argument passing (a historically fragile area) -----------------
//   Runtime checks, because the recurring failures here were silent miscompiles
//   (a dispatch resolving to `undefined`, a `&mut` lowering to broken JS) that a
//   compile-only test would pass. Covers: generic-bounded value dispatch
//   (roadmap Tier 1.2 / M2), a method routing its own generic into a nested call
//   (Bug C / B5), auto-deref through a view-returning call (B2), and `&`/`&mut`
//   argument passing (C5 / R8). Two open cases are pinned as ignored tests.

#[test]
fn generic_bounded_value_method_dispatch() {
    // A trait method called on a value of a generic-bounded type (`x: T: Display`)
    // dispatches to the concrete impl per monomorphization, at each call type —
    // not the abstract trait method (which would print `undefined`). Roadmap 1.2.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;
        fun describe<T: Display>(x: T): str { x.to_string() }
        fun main() {
            print(describe(42));
            print(describe("hi"));
        }
        "#,
        "42\nhi\n",
    );
}

#[test]
fn generic_bounded_value_operator_dispatch() {
    // `==` on a value of a generic-bounded type (`a: T: PartialEq`) re-resolves to
    // the concrete impl per monomorphization — for a primitive (native `===`) and
    // a `str`. Roadmap 1.2 / generic-equality.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;
        fun same<T: PartialEq>(a: T, b: T): bool { a == b }
        fun main() {
            if same(3, 3) { print("y") } else { print("n") }
            if same(1, 2) { print("y") } else { print("n") }
            if same("a", "a") { print("y") } else { print("n") }
        }
        "#,
        "y\nn\ny\n",
    );
}

#[test]
fn method_routes_own_generic_to_nested_call() {
    // A method on a generic impl passes the impl's type parameter into a *nested*
    // generic call (`format(self.v)`), which must monomorphize for the concrete
    // element at each instantiation (Bug C / B5). The receiver's `T` reaches the
    // nested call through the field access + the inherited substitution.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::{ Display, format };
        struct Wrap<T: Display> { v: T }
        impl Wrap<type T: Display> {
            fun render(self): str { format(self.v) }
        }
        fun main() {
            print(Wrap { v = 7 }.render());
            print(Wrap { v = "hi" }.render());
        }
        "#,
        "7\nhi\n",
    );
}

#[test]
fn auto_deref_through_view_returning_call() {
    // Field and method access on a `borrows` view-returning call: `o.slot().n` and
    // `o.slot().get()` auto-deref the returned `&mut Inner` to reach the inner
    // struct's member (backlog B2). Locks the behavior in (a regression would make
    // the access miss the deref).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct Inner { n: i32 }
        impl Inner { fun get(self): i32 { self.n } }
        struct Outer { inner: Inner }
        impl Outer { fun slot(&mut self): &mut Inner borrows self { &mut self.inner } }
        fun main() {
            mut o = Outer { inner = Inner { n = 5 } };
            print(o.slot().n);
            print(o.slot().get());
        }
        "#,
        "5\n5\n",
    );
}

#[test]
fn mut_view_argument_mutates_through_call_chain() {
    // R8: a `&mut` argument is passed as an explicit `&mut place` and mutates the
    // caller's place; forwarding the view to a further call (`via` -> `bump`)
    // re-borrows it and keeps writing through. Runtime, so the `(base, key)`
    // place-write is exercised end to end.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun bump(x: &mut i32) { x += 1; }
        fun via(y: &mut i32) { bump(y); }
        fun main() {
            mut a = 0;
            bump(&mut a);
            print(a);
            via(&mut a);
            print(a);
        }
        "#,
        "1\n2\n",
    );
}

#[test]
fn mut_view_as_method_argument_mutates() {
    // A `&mut` parameter on a *non-`self`* method argument (`target`) mutates the
    // caller's place across repeated calls — distinct from the implicitly-borrowed
    // `self` receiver. C5 / R8.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct Counter { n: i32 }
        impl Counter { fun add_into(self, target: &mut i32) { target += self.n; } }
        fun main() {
            mut total = 10;
            let c = Counter { n = 5 };
            c.add_into(&mut total);
            c.add_into(&mut total);
            print(total);
        }
        "#,
        "20\n",
    );
}

#[test]
fn mixed_value_view_and_own_arguments() {
    // One call mixing the three argument modes: a by-value `base` (read), a `&mut`
    // view `acc` (writes through to the caller), and an `own scratch` (a private
    // mutable copy the caller never sees). Each must keep its own semantics.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun combine(base: i32, acc: &mut i32, own scratch: i32): i32 {
            acc += base;
            scratch += 100;
            scratch
        }
        fun main() {
            mut a = 1;
            let s = combine(2, &mut a, 7);
            print(a); // 3 — written through the view
            print(s); // 107 — the own copy
        }
        "#,
        "3\n107\n",
    );
}

#[test]
fn reject_bare_value_to_shared_reference_param() {
    // R8 for a shared `&` parameter (the complement of `r8_reject_implicit_borrow`,
    // which covers `&mut`): a bare value place is rejected — pass `& <place>`.
    assert_fails(
        r#"
        fun read_it(x: &i32): i32 { *x }
        fun main() { let a = 5; let n = read_it(a); }
        "#,
    );
}

#[test]
fn generic_mut_view_parameter_writes_through() {
    // A generic `&mut T` view now behaves exactly like a concrete `&mut <T>`. For a
    // scalar pointee (`i32`, `f64`, `str`, `u32`) the read/write goes through the
    // `(base, key)` place-write, decided at monomorphization (the analyzer can't,
    // with `T` abstract — it emitted the aggregate `Object.assign`, leaving `a`
    // unchanged). For an aggregate pointee it stays the in-place copy.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun replace<T>(slot: &mut T, value: T) { slot = value; }
        fun main() {
            mut a = 1;
            replace(&mut a, 9);
            print(a);             // 9 — i32 written through
            mut f = 1.0;
            replace(&mut f, 2.5);
            print(f);             // 2.5 — f64
            mut s = "hi";
            replace(&mut s, "hey");
            print(s);             // hey — str
        }
        "#,
        "9\n2.5\nhey\n",
    );
}

#[test]
fn generic_mut_view_reads_and_swaps() {
    // Reading through a generic `&mut T` view (`*a`) and a `swap<T>` that both reads
    // and writes both views — the place-read `slot[0][slot[1]]` is also picked at
    // monomorphization for a scalar `T`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;
        fun peek<T: Display>(slot: &mut T): str { (*slot).to_string() }
        fun swap<T>(a: &mut T, b: &mut T) { let t = *a; a = *b; b = t; }
        fun main() {
            mut a = 5;
            print(peek(&mut a));
            mut x = 1;
            mut y = 2;
            swap(&mut x, &mut y);
            print(x);
            print(y);
        }
        "#,
        "5\n2\n1\n",
    );
}

#[test]
fn generic_mut_view_of_a_generic_local() {
    // The caller side: a `&mut` of a *generic-typed local* (`mut local = x` where
    // `x: T`) forwarded to another generic view parameter. The local must be boxed
    // and the reference must build the `(base, key)` pair when `T` resolves to a
    // scalar here — decided in the transformer (`generic_referenced_roots`), since
    // the analyzer saw `T` abstract. An aggregate `T` stays unboxed. (Before the
    // fix the scalar case crashed: `slot[0][slot[1]]` on an unboxed value.)
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun inner<T>(slot: &mut T, value: T) { slot = value; }
        fun outer<T>(x: T, value: T): T { mut local = x; inner(&mut local, value); local }
        struct P { x: i32 }
        fun main() {
            print(outer(1, 9));                       // scalar local -> 9
            print(outer(P { x = 1 }, P { x = 9 }).x); // aggregate local -> 9
        }
        "#,
        "9\n9\n",
    );
}

#[test]
fn generic_mut_view_aggregate_pointee_copies_in_place() {
    // The aggregate side of the same parameter: a generic `&mut T` where `T`
    // resolves to a struct rebinds via the in-place copy (not a `(base, key)`
    // write), so the caller's value updates. Guards that the scalar fix didn't
    // change the aggregate path.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct P { x: i32 }
        fun replace<T>(slot: &mut T, value: T) { slot = value; }
        fun main() {
            mut p = P { x = 1 };
            replace(&mut p, P { x = 9 });
            print(p.x);
        }
        "#,
        "9\n",
    );
}

#[test]
fn a_trait_annotated_binding_dispatches_on_its_concrete_type() {
    // SUPERSEDED BY B161 (was `bare_trait_value_method_call_is_rejected`).
    // `let x: Display = 5` used to have no concrete type to dispatch to — the
    // call lowered to the empty abstract method (`undefined`), then was made a
    // clean compile error at the annotation (B4/B72). B161 keeps the hole shut
    // by a different route: the annotation is a CONSTRAINT, not the type, so
    // `x` is an `i32` and `x.to_string()` is `i32`'s. There was never a trait
    // value here to dispatch on, and now there is not even the appearance of
    // one. The legitimate use of a bare trait as a TYPE is still nowhere — a
    // bound (`<T: Display>`) is exercised by `generic_dispatch_to_extern_impl`
    // et al., and every other value position still refuses (see
    // `traits.rs`'s narrowing pins).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;
        fun main() {
            let x: Display = 5;
            print(x.to_string());
        }
        main();
        "#,
        "5\n",
    );
}

#[test]
fn trait_default_self_dispatch_still_runs() {
    // The flip side of the rejection: inside a trait *default* body a `Self`
    // receiver — including a chain through a `Self`-returning method and a
    // non-`self` `Self`-typed parameter — is legitimate and re-dispatches to the
    // concrete type at codegen. Guards that the bare-trait-value check doesn't
    // catch these.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Stepper {
            fun step(self): i32;
            fun twice(self): i32 { self.step() + self.step() }
            fun plus(self, other: Self): i32 { self.twice() + other.step() }
        }
        struct One {}
        impl One with Stepper { fun step(self): i32 { 1 } }
        fun main() {
            let a = One {};
            let b = One {};
            print((a).plus(b));
        }
        "#,
        "3\n",
    );
}

// --- B6: inferred-element list, closure-param field access -------------------

#[test]
fn inferred_list_closure_param_field_access() {
    // A `List::new()` + `push` list has its element type inferred from `push`,
    // which lands (via a `SlotUnification`) *after* a following `map`/`filter`
    // would resolve. A method on such a receiver now defers while a `push`/`run`
    // to fill the slot is still pending, so the closure parameter types against
    // the concrete element and a field access on it works — no `mut xs: List<P>`
    // annotation needed (backlog B6 / roadmap Tier 1.2). Parity with a literal
    // list.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct P { x: i32 }
        fun main() {
            mut xs = List::new();
            xs.push(P { x = 10 });
            xs.push(P { x = 20 });
            let big = xs.filter(|p| p.x > 15);
            print(big.len());
            let labels = xs.map(|p| p.x);
            print(labels.len());
        }
        "#,
        "1\n2\n",
    );
}

#[test]
fn inferred_list_never_pushed_still_resolves() {
    // The deferral must not strand a `List::new()` that is *never* pushed: with no
    // pending `SlotUnification`, its methods resolve immediately (element stays
    // `Unknown`/`any`) rather than deferring forever.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let xs = List::new();
            print(xs.len());
            let ys = xs.map(|n| 1);
            print(ys.len());
        }
        "#,
        "0\n0\n",
    );
}

#[test]
fn inline_match_on_method_result_field_access() {
    // An inline `match` on a method call that returns `Option<element>`
    // (`match xs.get(0) { Some(let p) => p.x }`) typed its capture `p` only on a
    // late pass; the field accessor on `p` was woken by that resolution but the
    // fixpoint's backstop branch could terminate *before* running the woken
    // constraint (its `wake_ready` result was ignored). The loop now continues
    // while a wake is pending, so the access resolves. Worked when bound to a
    // `let` first (an extra pass) — now works inline too, for `get` and `pop`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        struct P { x: i32 }
        fun main() {
            mut xs = List::new();
            xs.push(P { x = 42 });
            match xs.get(0) {
                Some(let p) => print(p.x),
                None => print(0),
            }
            match xs.pop() {
                Some(let p) => print(p.x),
                None => print(0),
            }
        }
        "#,
        "42\n42\n",
    );
}

#[test]
fn impl_binder_inherits_struct_bound() {
    // `impl Wrapper<type T>` omits the bound the struct declares (`struct
    // Wrapper<T: Greeter>`). The impl can only ever apply to a `Wrapper`, whose
    // existence already requires `T: Greeter`, so the binder *inherits* that
    // bound — and a trait method call on the `T`-typed field resolves, exactly as
    // if `impl Wrapper<type T: Greeter>` had been written.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Greeter { fun greet(self): str; }
        struct Hello { name: str }
        impl Hello with Greeter { fun greet(self): str { "hi " + self.name } }
        struct Wrapper<T: Greeter> { inner: T }
        impl Wrapper<type T> {
            fun run(self): str { (self.inner).greet() }
        }
        fun main() {
            print(Wrapper { inner = Hello { name = "x" } }.run());
        }
        "#,
        "hi x\n",
    );
}

#[test]
fn impl_binder_inherits_multiple_bounds() {
    // A multi-bound declared parameter (`T: A + B`) keeps *both* bounds when
    // inherited: the extra bounds hang off the same constraint id the binder
    // reuses, so methods from either trait resolve on the field.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Named { fun name(self): str; }
        trait Aged { fun age(self): i32; }
        struct Person { n: str, a: i32 }
        impl Person with Named { fun name(self): str { self.n } }
        impl Person with Aged { fun age(self): i32 { self.a } }
        struct Card<T: Named + Aged> { who: T }
        impl Card<type T> {
            fun render(self): str { (self.who).name() }
            fun years(self): i32 { (self.who).age() }
        }
        fun main() {
            let card = Card { who = Person { n = "Ada", a = 36 } };
            print(card.render());
            print(card.years());
        }
        "#,
        "Ada\n36\n",
    );
}

#[test]
fn impl_binder_inherits_per_position_with_multiple_params() {
    // Two declared parameters with *different* bounds — the inherited constraint
    // is matched to the binder by position, not conflated.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Named { fun name(self): str; }
        trait Aged { fun age(self): i32; }
        struct Tag { n: str }
        impl Tag with Named { fun name(self): str { self.n } }
        struct Years { y: i32 }
        impl Years with Aged { fun age(self): i32 { self.y } }
        struct Pair<A: Named, B: Aged> { left: A, right: B }
        impl Pair<type A, type B> {
            fun label(self): str { (self.left).name() }
            fun count(self): i32 { (self.right).age() }
        }
        fun main() {
            let pair = Pair { left = Tag { n = "Ada" }, right = Years { y = 7 } };
            print(pair.label());
            print(pair.count());
        }
        "#,
        "Ada\n7\n",
    );
}

#[test]
fn impl_binder_mixes_explicit_and_inherited_bounds() {
    // One binder restates its bound explicitly, the other infers it — both must
    // resolve. The explicit one already worked; this pins that adding inheritance
    // for the other did not break the mixed form.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Named { fun name(self): str; }
        trait Aged { fun age(self): i32; }
        struct Tag { n: str }
        impl Tag with Named { fun name(self): str { self.n } }
        struct Years { y: i32 }
        impl Years with Aged { fun age(self): i32 { self.y } }
        struct Pair<A: Named, B: Aged> { left: A, right: B }
        impl Pair<type A: Named, type B> {
            fun label(self): str { (self.left).name() }
            fun count(self): i32 { (self.right).age() }
        }
        fun main() {
            let pair = Pair { left = Tag { n = "Ada" }, right = Years { y = 7 } };
            print(pair.label());
            print(pair.count());
        }
        "#,
        "Ada\n7\n",
    );
}

#[test]
fn impl_binder_inherits_enum_bound() {
    // Inheritance works for an enum subject too, not just structs.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Greeter { fun greet(self): str; }
        struct Hello { name: str }
        impl Hello with Greeter { fun greet(self): str { "hi " + self.name } }
        enum Box<T: Greeter> { Full(T), Empty }
        impl Box<type T> {
            fun shout(self): str {
                match self {
                    Box::Full(let inner) => inner.greet(),
                    Box::Empty => "empty",
                }
            }
        }
        fun main() {
            print(Box::Full(Hello { name = "x" }).shout());
        }
        "#,
        "hi x\n",
    );
}

#[test]
fn impl_binder_without_a_declared_bound_stays_unconstrained() {
    // Inheritance only borrows a bound the subject actually declares. An
    // unconstrained `struct Plain<T>` confers nothing, so a trait method call on
    // the `T`-typed field must still be rejected — the fix must not invent bounds.
    assert_fails(
        r#"
        import std::io::print;
        trait Greeter { fun greet(self): str; }
        struct Plain<T> { inner: T }
        impl Plain<type T> {
            fun run(self): str { (self.inner).greet() }
        }
        fun main() {
            print(0);
        }
        "#,
    );
}

#[test]
fn impl_binder_inherits_bound_from_a_later_declared_struct() {
    // The same program as `impl_binder_inherits_struct_bound`, but with the
    // struct declared *after* the impl. The walk registers the binder
    // unbounded and retrofits the struct's bound just before solving, once
    // every declaration exists — declaration order no longer matters.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Greeter { fun greet(self): str; }
        struct Hello { name: str }
        impl Hello with Greeter { fun greet(self): str { "hi " + self.name } }
        impl Wrapper<type T> {
            fun run(self): str { (self.inner).greet() }
        }
        struct Wrapper<T: Greeter> { inner: T }
        fun main() {
            print(Wrapper { inner = Hello { name = "x" } }.run());
        }
        "#,
        "hi x\n",
    );
}

#[test]
fn impl_binder_inherits_multiple_bounds_from_a_later_declared_struct() {
    // The deferred retrofit carries MULTI-bounds too: `T: Greeter + Counter`
    // declared after the impl, methods from both traits resolving.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Greeter { fun greet(self): str; }
        trait Counter { fun count(self): i32; }
        struct Hello { name: str }
        impl Hello with Greeter { fun greet(self): str { "hi " + self.name } }
        impl Hello with Counter { fun count(self): i32 { self.name.len() } }
        impl Wrapper<type T> {
            fun describe(self): str {
                (self.inner).greet()
            }
            fun tally(self): i32 {
                (self.inner).count()
            }
        }
        struct Wrapper<T: Greeter + Counter> { inner: T }
        fun main() {
            let wrapped = Wrapper { inner = Hello { name = "xy" } };
            print(wrapped.describe());
            print(wrapped.tally());
        }
        "#,
        "hi xy\n2\n",
    );
}

#[test]
fn impl_binder_inherits_bound_from_a_later_declared_enum() {
    // Enum subjects inherit through the same deferred path as structs.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Greeter { fun greet(self): str; }
        struct Hello { name: str }
        impl Hello with Greeter { fun greet(self): str { "hi " + self.name } }
        impl Holder<type T> {
            fun open(self): str {
                match self {
                    Holder::Item(let inner) => inner.greet(),
                }
            }
        }
        enum Holder<T: Greeter> {
            Item(T),
        }
        fun main() {
            print(Holder::Item(Hello { name = "e" }).open());
        }
        "#,
        "hi e\n",
    );
}

#[test]
fn a_boundless_trait_argument_binder_inherits_the_traits_bound() {
    // `with DescribeInto<type S>` omits the bound; the TRAIT declares
    // `S: Sink`, so the binder inherits it — the subject-binder rule applied
    // to the with-clause.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        trait Sink { fun put(self, value: i32); }
        struct Collector { total: Shared<i32> }
        impl Collector with Sink {
            fun put(self, value: i32) {
                self.total.write() = self.total.read() + value;
            }
        }
        trait DescribeInto<S: Sink> {
            fun describe_into(self, sink: S);
        }
        struct Point { x: i32 }
        impl Point with DescribeInto<type S> {
            fun describe_into(self, sink: S) {
                sink.put(self.x);
            }
        }
        fun main() {
            let point = Point { x = 5 };
            let collector = Collector { total = Shared::new(0) };
            point.describe_into(collector);
            print(collector.total.read());
        }
        "#,
        "5\n",
    );
}

#[test]
fn subject_and_trait_argument_binders_compose_on_one_impl() {
    // `impl Box<type T> with DescribeInto<type S: Sink>` — the receiver binds
    // T, the argument binds S, one call resolves both.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        trait Sink { fun put(self, value: i32); }
        struct Collector { total: Shared<i32> }
        impl Collector with Sink {
            fun put(self, value: i32) {
                self.total.write() = self.total.read() + value;
            }
        }
        trait Sized2 { fun size(self): i32; }
        struct Pair { a: i32, b: i32 }
        impl Pair with Sized2 { fun size(self): i32 { 2 } }
        trait DescribeInto<S> {
            fun describe_into(self, sink: S);
        }
        struct Box2<T: Sized2> { inner: T }
        impl Box2<type T> with DescribeInto<type S: Sink> {
            fun describe_into(self, sink: S) {
                sink.put((self.inner).size());
            }
        }
        fun main() {
            let boxed = Box2 { inner = Pair { a = 1, b = 2 } };
            let collector = Collector { total = Shared::new(40) };
            boxed.describe_into(collector);
            print(collector.total.read());
        }
        "#,
        "42\n",
    );
}

#[test]
fn async_trait_method_through_generic_bound_auto_awaits() {
    // An inferred-async trait method (`fetch` awaits) dispatched through a generic
    // bound (`self.inner: T`, `T: Fetcher`). The call graph used to mis-resolve the
    // dispatch to the trait's *signature* (a bodyless method, never async — the
    // dispatch is keyed by the call id, which `resolve_target` only consulted for
    // `OnType`), so the enclosing `run` was left non-`async` while the transformer,
    // resolving the concrete async impl, still inserted the `await` — `await` inside
    // a non-async function, invalid JS that crashed at load. Async-ness now
    // propagates through the dispatch's candidate impls, so `run` (and its caller
    // `main`) are async and the program runs.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        [extern("Promise.resolve")]
        async external fun resolved(value: str): str;
        trait Fetcher { fun fetch(self): str; }
        struct Remote { tag: str }
        impl Remote with Fetcher {
            fun fetch(self): str { await resolved(self.tag) }
        }
        struct Wrapper<T: Fetcher> { inner: T }
        impl Wrapper<type T> {
            fun run(self): str { (self.inner).fetch() }
        }
        fun main() {
            print(Wrapper { inner = Remote { tag = "hi" } }.run());
        }
        "#,
        "hi\n",
    );
}

#[test]
fn async_impl_through_generic_bound_propagates_transitively() {
    // The impl method is async *transitively* — it doesn't `await` itself, it calls
    // an async function — so its async-ness is only settled by the fixpoint. The
    // dispatch must pick that up after propagation, not just from a direct `await`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        [extern("Promise.resolve")]
        async external fun resolved(value: str): str;
        fun load(tag: str): str { await resolved(tag) }
        trait Fetcher { fun fetch(self): str; }
        struct Remote { tag: str }
        impl Remote with Fetcher {
            fun fetch(self): str { load(self.tag) }
        }
        struct Wrapper<T: Fetcher> { inner: T }
        impl Wrapper<type T> {
            fun run(self): str { (self.inner).fetch() }
        }
        fun main() {
            print(Wrapper { inner = Remote { tag = "hey" } }.run());
        }
        "#,
        "hey\n",
    );
}

#[test]
fn mixed_async_and_sync_impls_through_generic_bound_both_run() {
    // Two impls of one trait — one async, one sync — both reached through the bound.
    // The dispatch is conservatively async (some candidate impl awaits), so even the
    // sync instance compiles to an async function; awaiting its non-promise result is
    // a JS no-op, and both instantiations run correctly.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        [extern("Promise.resolve")]
        async external fun resolved(value: str): str;
        trait Fetcher { fun fetch(self): str; }
        struct Remote { tag: str }
        impl Remote with Fetcher { fun fetch(self): str { await resolved(self.tag) } }
        struct Local { tag: str }
        impl Local with Fetcher { fun fetch(self): str { self.tag } }
        struct Wrapper<T: Fetcher> { inner: T }
        impl Wrapper<type T> { fun run(self): str { (self.inner).fetch() } }
        fun main() {
            print(Wrapper { inner = Remote { tag = "remote" } }.run());
            print(Wrapper { inner = Local { tag = "local" } }.run());
        }
        "#,
        "remote\nlocal\n",
    );
}

#[test]
fn async_trait_default_body_through_generic_bound_auto_awaits() {
    // The async method is the trait's *default* body (the impl doesn't override it),
    // dispatched through the bound. The candidate is the trait default, not an impl
    // member — so candidate resolution must consider the trait's own declarations.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        [extern("Promise.resolve")]
        async external fun resolved(value: str): str;
        trait Greeter {
            fun name(self): str;
            fun greet(self): str { await resolved(self.name()) }
        }
        struct Hello { who: str }
        impl Hello with Greeter { fun name(self): str { self.who } }
        struct Wrapper<T: Greeter> { inner: T }
        impl Wrapper<type T> { fun run(self): str { (self.inner).greet() } }
        fun main() {
            print(Wrapper { inner = Hello { who = "ada" } }.run());
        }
        "#,
        "ada\n",
    );
}

#[test]
fn sync_method_through_generic_bound_is_not_made_async() {
    // The precision guard: a generic dispatch whose trait has *no* async impl must
    // not become async. Asserted structurally — the emitted JS has no `async`/`await`
    // anywhere — so an over-eager propagation (e.g. matching an async method of the
    // same name in an unrelated trait) would fail here, not just slip past `runs`.
    let js = compile(
        r#"
        import std::io::print;
        trait Greeter { fun greet(self): str; }
        struct Hello { name: str }
        impl Hello with Greeter { fun greet(self): str { "hi " + self.name } }
        struct Wrapper<T: Greeter> { inner: T }
        impl Wrapper<type T> { fun run(self): str { (self.inner).greet() } }
        fun main() { print(Wrapper { inner = Hello { name = "x" } }.run()); }
        "#,
    )
    .expect("compiles");
    assert!(
        !js.contains("async") && !js.contains("await"),
        "a purely-sync generic dispatch must not be made async:\n{js}"
    );
}

// ---------------------------------------------------------------------------
// B141: a postfix applied to an `await` must parenthesise the await.
//
// The emitter rendered an await as the prefix form `await (<operand>)` — it
// parenthesised the OPERAND but never the whole await-expression — and the
// postfix arms rendered their subject with no parens of their own. Member
// access and call bind TIGHTER than the `await` unary, so `await (f()).x`
// parsed as `await ((f()).x)`: the property was read off the PROMISE and the
// program silently got `undefined`, with a clean `vilan check` and exit 0.
//
// These pin the shape class, one per postfix form, by RUNNING the emitted
// program and asserting the value — the miscompile is invisible to
// `assert_compiles`, which is exactly how it survived into released
// toolchains. Every helper below awaits, so every call to it is implicitly
// awaited; the async transparency the language promises is precisely the
// inline spelling these pin.

/// The shared preamble: async helpers whose calls are implicitly awaited.
const AWAIT_POSTFIX_PRELUDE: &str = r#"
        import std::io::print;
        [extern("Promise.resolve")]
        async external fun resolved(value: i32): i32;
        struct Row { id: i32, name: str }
        struct Boxed { n: i32 }
        impl Boxed { fun doubled(self): i32 { self.n * 2 } }
        fun fetch_row(): Row { resolved(0); Row { id = 7, name = "seven" } }
        fun fetch_list(): List<i32> { resolved(0); [10, 20, 30] }
        fun fetch_num(): i32 { resolved(0); 5 }
        fun fetch_boxed(): Boxed { resolved(0); Boxed { n = 21 } }
        fun fetch_maker(): || i32 { resolved(0); || 99 }
"#;

fn await_postfix_program(body: &str) -> String {
    format!("{AWAIT_POSTFIX_PRELUDE}        fun main() {{ {body} }}")
}

#[test]
fn a_field_off_an_implicitly_awaited_call_reads_the_value() {
    // B141's headline shape. Emitted `await (fetch_row())[0]` — `undefined`.
    assert_compiles_and_runs(&await_postfix_program("print(fetch_row().id);"), "7\n");
}

#[test]
fn a_method_off_an_implicitly_awaited_call_reads_the_value() {
    // A built-in method lowers to a `.length` property access, so it is the
    // field shape again: emitted `await (fetch_list()).length` — `undefined`.
    assert_compiles_and_runs(&await_postfix_program("print(fetch_list().len());"), "3\n");
}

#[test]
fn a_call_of_an_implicitly_awaited_callable_invokes_the_value() {
    // Calling the closure an async function returned. Emitted
    // `await (fetch_maker())()` — which is `await ((fetch_maker())())`, a call
    // of the PROMISE: this shape did not go silent, it threw
    // `TypeError: fetch_maker(...) is not a function`. Not in B141's original
    // probe; found widening the class from "postfix" to "call as well".
    assert_compiles_and_runs(&await_postfix_program("print(fetch_maker()());"), "99\n");
}

#[test]
fn a_postfix_chain_off_an_implicitly_awaited_call_reads_the_value() {
    // Two postfix links off one await (`.name` then `.len()`). Threw
    // `Cannot read properties of undefined` — the first link already had the
    // promise, so the second had `undefined`.
    assert_compiles_and_runs(
        &await_postfix_program("print(fetch_row().name.len());"),
        "5\n",
    );
}

#[test]
fn a_field_off_an_explicitly_awaited_task_reads_the_value() {
    // The bug was NOT confined to the implicit await: a hand-written
    // `(await pending).id` — parens and all — emitted `await (pending)[0]` and
    // read `undefined`. The user's own parentheses were dropped, then the
    // subject was re-rendered without them. Also outside B141's original probe.
    assert_compiles_and_runs(
        &await_postfix_program("let pending = async fetch_row(); print((await pending).id);"),
        "7\n",
    );
}

#[test]
fn a_subscript_off_an_implicitly_awaited_call_reads_the_value() {
    // This shape was already CORRECT before the fix — but only by accident:
    // `__at()` wraps the await in a call ARGUMENT, which parenthesises it for
    // free. Pinned so it cannot regress silently if that helper is ever
    // inlined to a bare `[..]`, which would put the await straight back into
    // postfix-subject position.
    assert_compiles_and_runs(&await_postfix_program("print(fetch_list()[0]);"), "10\n");
}

#[test]
fn a_user_method_off_an_implicitly_awaited_call_reads_the_value() {
    // The other accident: a user method lowers to a FREE function call, so the
    // await lands in argument position and is parenthesised for free. Pinned
    // for the same reason as the subscript.
    assert_compiles_and_runs(
        &await_postfix_program("print(fetch_boxed().doubled());"),
        "42\n",
    );
}

#[test]
fn binding_an_awaited_call_before_a_postfix_reads_the_value() {
    // The CONTROL: the bound spelling was always correct, which is why std and
    // the whole corpus — which bind — never saw B141. If this ever goes red the
    // failure is somewhere else entirely.
    assert_compiles_and_runs(
        &await_postfix_program(
            "let row = fetch_row(); let l = fetch_list(); print(row.id); print(l.len());",
        ),
        "7\n3\n",
    );
}

#[test]
fn an_await_in_operand_position_is_not_gratuitously_parenthesised() {
    // The counter-pin for the fix's blast radius. `await` binds TIGHTER than
    // every binary operator, so an await in operand position needs no parens
    // and must not acquire any — the fix is context-aware, not a blanket
    // `(await x)`. Pinned on the emitted bytes because that is the property at
    // risk: a blanket wrap would still run correctly and move every golden.
    let js = compile(&await_postfix_program(
        "print(fetch_num() + 1); print(fetch_num() > 3);",
    ))
    .expect("compiles");
    assert!(
        js.contains("await (fetch_num()) + 1") && js.contains("await (fetch_num()) > 3"),
        "an await in binary-operand position must stay unwrapped:\n{js}"
    );
    assert!(
        !js.contains("(await (fetch_num()))"),
        "an await in binary-operand position must not be wrapped:\n{js}"
    );
}

#[test]
fn a_postfix_off_an_await_is_parenthesised_in_the_emitted_js() {
    // The structural twin of the execution pins: the emitted bytes, so a
    // regression is legible as `await (f()).x` rather than only as a wrong
    // number. `(await (…))[0]` — the inner parens are the await's own operand
    // parens, the outer are the fix.
    let js = compile(&await_postfix_program("print(fetch_row().id);")).expect("compiles");
    assert!(
        js.contains("(await (fetch_row()))[0]"),
        "the await must be parenthesised under a postfix:\n{js}"
    );
}

#[test]
fn generic_element_serialized_in_a_closure_through_a_bounded_method() {
    // A closure passed to a generic method (`feed.each(|T| ..)`) on a parameterized-bound
    // receiver (`F: Feed<T>`), serializing the element `T` inside the closure. Two gaps
    // used to break this: the closure parameter lost its `T: Json` bound — a compile error
    // ("cannot call method 'to_json' on T") — and `T`, which appears *only* in the bound
    // `F: Feed<T>`, was never derived from the concrete `Nums: Feed<i32>` at the call site,
    // so `to_json` monomorphized to the empty abstract method and yielded `undefined`.
    // Both are fixed (the parameterized-bound substitution in the `Type::Generic` method
    // arm, and the derive-from-bound step in `resolve_call_subject`).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::Json;
        trait Feed<T> { fun each(self, observer: |T| void); }
        struct Nums {}
        impl Nums with Feed<i32> {
            fun each(self, observer: |i32| void) { observer(7); observer(9); }
        }
        fun pump<T: Json, F: Feed<T>>(feed: F, out: |str| void) {
            feed.each(|value| out(value.to_json()))
        }
        fun main() { pump(Nums {}, |s| print(s)); }
        "#,
        "7\n9\n",
    );
}

#[test]
fn generic_source_element_serialized_in_a_sub_closure() {
    // The reactive shape the fix unblocks: forward a `Source<T>`'s values, serialized
    // inside the `sub` closure, where `T` appears only in the `S: Source<T>` bound.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::Json;
        import std::reactive::{ Source, Signal, Subscription };
        fun forward<T: Json, S: Source<T>>(source: S, out: |str| void): Subscription {
            source.sub(|value| out(value.to_json()))
        }
        fun main() {
            let s = Signal::new(7);
            let _ = forward(s, |json| print(json));
            s.set(9);
        }
        "#,
        "7\n9\n",
    );
}

#[test]
fn generic_element_type_derived_from_a_parameterized_bound() {
    // A struct payload `T` (not a scalar) crosses the same paths: the element flows
    // through the closure and a `[derive(Json)]` `to_json`, and `T` is derived from the
    // bound. Pins that the fix threads a concrete *aggregate* type, not just `i32`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::Json;
        trait Feed<T> { fun each(self, observer: |T| void); }
        [derive(Json)]
        struct Point { x: i32, y: i32 }
        struct Points {}
        impl Points with Feed<Point> {
            fun each(self, observer: |Point| void) { observer(Point { x = 1, y = 2 }); }
        }
        fun dump<T: Json, F: Feed<T>>(feed: F) {
            feed.each(|point| print(point.to_json()))
        }
        fun main() { dump(Points {}); }
        "#,
        "{\"x\":1,\"y\":2}\n",
    );
}

#[test]
fn generic_bound_derivation_through_a_method_call() {
    // The same fix on the *method* path (`bind_method_own_generics`): a struct method
    // `<T: Json, S: Source<T>>` whose `T` appears only in the bound, serializing the
    // element in a `sub` closure. Called as `sink.forward(signal, ..)`, `T` is derived
    // from the concrete signal's `Source` impl — the shape `examples/rpc`'s `expose` uses.
    // The source argument is *inferred* (`let s = Signal::new(7)`, no annotation), so its
    // type lands only after the call is first seen; `resolve_method_call` defers while the
    // bound-owner is unresolved and re-derives on a later pass (mirroring the free-function
    // path), so the inferred case works too.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::Json;
        import std::reactive::{ Source, Signal, Subscription };
        struct Sink {}
        impl Sink {
            fun forward<T: Json, S: Source<T>>(self, source: S, out: |str| void): Subscription {
                source.sub(|value| out(value.to_json()))
            }
        }
        fun main() {
            let s = Signal::new(7);
            let _ = Sink {}.forward(s, |json| print(json));
            s.set(9);
        }
        "#,
        "7\n9\n",
    );
}

#[test]
fn owner_take_disposes_a_mapped_and_a_root_subscription() {
    // Pins `vilan/test/reactive.js`'s reachable miscompilation as *observable* runtime
    // behaviour — the golden alone proved an unreliable gate (it drifted stale), so an
    // executed assertion is the stronger pin. `Owner::take<T: Disposable>` (an *unparameterized*
    // bound) stores `|| item.dispose()` in a cleanup closure for later. Two `take` sites are
    // needed to trigger it: `take(mapped.sub(..))` where `mapped = root.map(..)` resolves its
    // element *late* (through `map`'s generic), and `take(root.sub(..))` which resolves early.
    // The pre-fix analyzer bound the *mapped* site's `T` before its argument landed and
    // monomorphized that `take` to the empty abstract `Disposable::dispose` (the *root* site
    // stayed concrete), so disposing the owner never removed the mapped subscriber and it
    // leaked. reactive.js hides it (its owner is never disposed); here we dispose the owner,
    // so a leaked subscription keeps firing: pre-fix this printed a trailing `a=10`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Owner };
        fun main() {
            let owner = Owner::new();
            let count = Signal::new(0);
            let doubled = count.map(|n| n * 2);
            owner.take(doubled.sub(|n| print(i"a={n}")));   // mapped/late site
            owner.take(count.sub(|n| print(i"b={n}")));     // root/early site
            count.set(1);       // a=2, b=1
            owner.dispose();    // the *real* dispose must remove BOTH subscribers
            count.set(5);       // silent iff both disposed; leaks "a=10" if the mapped take went abstract
        }
        "#,
        "a=0\nb=0\na=2\nb=1\n",
    );
}

// === Reactive batching (proposal/reactive-batching.md) ============================

#[test]
fn lone_set_notifies_synchronously() {
    // Outside a `batch`, `set` notifies inline (eager) — a lone set fires its observers
    // before the next statement, exactly as before batching existed.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal };
        fun main() {
            let a = Signal::new(0);
            let _ = a.sub(|v| print(i"a={v}"));   // immediate: a=0
            a.set(1);                             // eager -> a=1 now
            print("after");
            a.set(2);                             // a=2
        }
        "#,
        "a=0\na=1\nafter\na=2\n",
    );
}

#[test]
fn batch_commits_value_immediately_but_defers_notification() {
    // Inside a `batch`, a root's value is committed at once (`s.get()` is fresh), but a
    // *derived* value recomputes only at the flush boundary — so mid-batch it is stale,
    // then settles. Pins the "defer notification, not the value" divergence.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, batch };
        fun main() {
            let s = Signal::new(0);
            let doubled = s.map(|n| n * 2);
            batch(|| {
                s.set(5);
                print(i"in-batch s={s.get()} doubled={doubled.get()}");   // s=5 fresh, doubled=0 stale
            });
            print(i"after doubled={doubled.get()}");                      // 10 (settled at flush)
        }
        "#,
        "in-batch s=5 doubled=0\nafter doubled=10\n",
    );
}

#[test]
fn batch_coalesces_a_multi_input_observer() {
    // A node fed by two inputs (hand-rolled `d = a + b`, recomputed when either changes)
    // recomputes with both inputs settled inside a `batch` — glitch-free. The `d` observer
    // fires once (11 -> 22), with no intermediate (a-new, b-old) reading.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, batch };
        fun main() {
            let a = Signal::new(1);
            let b = Signal::new(10);
            let d = Signal::new(a.get() + b.get());
            let _ = a.sub(|_| { d.set(a.get() + b.get()); });
            let _ = b.sub(|_| { d.set(a.get() + b.get()); });
            let _ = d.sub(|v| print(i"d={v}"));   // immediate: d=11
            batch(|| {
                a.set(2);
                b.set(20);
            });                                    // coalesced -> d=22 once
        }
        "#,
        "d=11\nd=22\n",
    );
}

#[test]
fn without_a_batch_a_multi_input_observer_glitches() {
    // The same graph without a `batch`: each eager `set` fires the observer, so it sees the
    // intermediate (a=2, b=10) state — the glitch (`d=12`) the batch above elides. Pins that
    // batching is what removes it (the opt-in boundary).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal };
        fun main() {
            let a = Signal::new(1);
            let b = Signal::new(10);
            let d = Signal::new(a.get() + b.get());
            let _ = a.sub(|_| { d.set(a.get() + b.get()); });
            let _ = b.sub(|_| { d.set(a.get() + b.get()); });
            let _ = d.sub(|v| print(i"d={v}"));   // d=11
            a.set(2);                              // d=12 (glitch: b still 10)
            b.set(20);                             // d=22
        }
        "#,
        "d=11\nd=12\nd=22\n",
    );
}

#[test]
fn batch_cascade_settles_in_one_flush() {
    // A linear cascade `a -> map -> map -> observer` settles to its final value in one flush
    // when the root is set inside a `batch` — the observer fires once with the fully-cascaded
    // value (20 -> 60), never an intermediate.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, batch };
        fun main() {
            let a = Signal::new(1);
            let b = a.map(|n| n + 1);      // b = a + 1
            let c = b.map(|n| n * 10);     // c = b * 10
            let _ = c.sub(|v| print(i"c={v}"));   // immediate: c=20
            batch(|| { a.set(5); });               // a=5 -> b=6 -> c=60
        }
        "#,
        "c=20\nc=60\n",
    );
}

#[test]
fn nested_batches_flush_at_the_outer_boundary() {
    // An inner `batch` does not flush (depth stays > 0) — notifications wait for the outermost
    // boundary and coalesce to the final value. `mid` prints before any observer fires.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, batch };
        fun main() {
            let a = Signal::new(0);
            let _ = a.sub(|v| print(i"a={v}"));   // immediate: a=0
            batch(|| {
                a.set(1);
                batch(|| {
                    a.set(2);
                });
                print("mid");        // inner batch did NOT flush -> no a-notify yet
                a.set(3);
            });                       // outer flush -> a=3 (once, final)
        }
        "#,
        "a=0\nmid\na=3\n",
    );
}

#[test]
fn dispose_in_a_batch_scrubs_the_pending_notify() {
    // A subscription disposed *after* its source was set in the same `batch` must not fire:
    // `dispose` scrubs the pending queue, so the enqueued notify is removed before the flush.
    // Pins the "disposed is silent" resolution (no `tick 1` from the batch, no `tick 2` after).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, batch };
        fun main() {
            let counter = Signal::new(0);
            let sub = counter.sub(|n| print(i"tick {n}"));   // immediate: tick 0
            batch(|| {
                counter.set(1);     // enqueues `sub`'s notify
                sub.dispose();      // scrubs it from the pending queue
            });                      // flush -> nothing
            print("done");
            counter.set(2);          // sub disposed -> silent
        }
        "#,
        "tick 0\ndone\n",
    );
}

// --- `Signal::update`: in-place mutation (A18, proposal/signal-update.md) ---
// The closure receives a writable view of the STORED value and the runtime
// notifies once, unconditionally, after it returns. `sync` is the `await`
// fence; the view obeys rule 3 like any other.

#[test]
fn update_mutates_a_list_in_place_and_a_later_get_sees_it() {
    // A18's headline case: a push through the view, no copy-transform-return.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::Signal;
        fun main() {
            let todos = Signal::new([1, 2]);
            todos.update(|&mut list| { list.push(5); });
            print(todos.get().len());
        }
        "#,
        "3\n",
    );
}

#[test]
fn update_generalizes_over_every_collection() {
    // The point of the design: one method serves `Map` and `Set` (and a user
    // struct) exactly as it serves `List` — no per-container twin.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::map::Map;
        import std::set::Set;
        import std::reactive::Signal;
        struct Counter { hits: i32 }
        fun main() {
            let scores: Signal<Map<str, i32>> = Signal::new(Map::new());
            scores.update(|&mut m| { m.insert("a", 1); m.insert("b", 2); });
            print(scores.get().len());

            let tags: Signal<Set<i32>> = Signal::new(Set::new());
            tags.update(|&mut s| { s.insert(7); });
            print(tags.get().len());

            let counter = Signal::new(Counter { hits = 0 });
            counter.update(|&mut c| { c.hits = 9; });
            print(counter.get().hits);
        }
        "#,
        "2\n1\n9\n",
    );
}

#[test]
fn update_over_a_scalar_signal_writes_through_the_view() {
    // The scalar leg. `Shared::write()` over a scalar pointee lowers to its
    // `(base, key)` pair, so the closure's `&mut i32` writes the cell rather
    // than a stray number (which crashed at runtime before the fix).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::Signal;
        fun main() {
            let count = Signal::new(1);
            count.update(|&mut n| { n = *n + 10; });
            print(count.get());
        }
        "#,
        "11\n",
    );
}

#[test]
fn update_notifies_exactly_once_per_call() {
    // Unconditional and single: two `update`s produce two notifications, one
    // each, after the closure returns — never per mutation inside it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Owner };
        fun main() {
            let owner = Owner::new();
            let xs = Signal::new([0]);
            owner.take(xs.sub(|list| print(i"len {list.len()}")));   // immediate: len 1
            xs.update(|&mut list| { list.push(1); list.push(2); });  // ONE notify, len 3
            xs.update(|&mut list| { list.push(3); });                // len 4
        }
        "#,
        "len 1\nlen 3\nlen 4\n",
    );
}

#[test]
fn update_notifies_even_when_the_closure_writes_nothing() {
    // Unconditional, deliberately: `update` matches `set`, which never
    // compares either. A no-op `mutate` still publishes.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Owner };
        fun main() {
            let owner = Owner::new();
            let xs = Signal::new([0]);
            owner.take(xs.sub(|list| print(i"len {list.len()}")));
            xs.update(|&mut list| { });
        }
        "#,
        "len 1\nlen 1\n",
    );
}

#[test]
fn update_coalesces_under_batch() {
    // `update` shares `set`'s notify half verbatim, so turn deferral and dedup
    // are inherited: two updates inside one `batch` settle as ONE notification
    // at the boundary, carrying the final value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Owner, batch };
        fun main() {
            let owner = Owner::new();
            let xs = Signal::new([0]);
            owner.take(xs.sub(|list| print(i"len {list.len()}")));   // immediate: len 1
            batch(|| {
                xs.update(|&mut list| { list.push(1); });
                xs.update(|&mut list| { list.push(2); });
                print("inside");
            });
        }
        "#,
        "len 1\ninside\nlen 3\n",
    );
}

#[test]
fn a_reentrant_get_inside_update_sees_the_in_progress_value() {
    // `mutate` writes STORAGE, so a read from inside the closure observes the
    // mutations made so far — uniformly for an aggregate and for a scalar (a
    // scalar view writes the same `(cell, "v")` slot `get` reads).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::Signal;
        fun main() {
            let xs = Signal::new([1, 2]);
            xs.update(|&mut list| {
                list.push(3);
                print(i"aggregate {xs.get().len()}");
            });
            let n = Signal::new(1);
            n.update(|&mut value| {
                value = *value + 10;
                print(i"scalar {n.get()}");
            });
        }
        "#,
        "aggregate 3\nscalar 11\n",
    );
}

#[test]
fn update_refuses_a_view_escaping_its_closure() {
    // Rule 3 holds for the callback's parameter exactly as for any other view:
    // storing it in a struct field is the ordinary escape error.
    assert_fails_with(
        r#"
        import std::reactive::Signal;
        struct Hold { slot: &mut List<i32> }
        fun main() {
            let xs = Signal::new([1]);
            xs.update(|&mut list| { let held = Hold { slot = list }; });
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn set_with_still_copies_and_transforms() {
    // `update` does not replace `set_with`: the transform form is unchanged,
    // and its `mut` copy is still a copy (the source list is untouched).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::Signal;
        fun main() {
            mut seed = [1, 2];
            let numbers = Signal::new(seed);
            numbers.set_with(|mut list| {
                list.push(5);
                list
            });
            print(numbers.get().len());
            print(seed.len());
            let count = Signal::new(1);
            count.set_with(|n| n + 4);
            print(count.get());
        }
        "#,
        "3\n2\n5\n",
    );
}

// --- the language mechanism `update` needed: a closure literal's parameters
// --- take the full parameter grammar (conventions included), not just `mut`.

#[test]
fn a_closure_parameter_takes_the_mut_view_convention() {
    // `|&mut x|` is the prefix spelling; the callee mutates the CALLER's value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun apply(target: &mut List<i32>, mutate: sync |&mut List<i32>| void) {
            mutate(target);
        }
        fun main() {
            mut data = [1, 2];
            apply(&mut data, |&mut list| { list.push(5); });
            print(data.len());
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_closure_parameter_takes_the_view_convention_from_its_type() {
    // The type-position spelling, inferred the same way a `fun` parameter's is.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun apply(target: &mut List<i32>, mutate: sync |&mut List<i32>| void) {
            mutate(target);
        }
        fun main() {
            mut data = [1, 2];
            apply(&mut data, |list: &mut List<i32>| { list.push(5); });
            print(data.len());
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_bare_closure_parameter_still_refuses_mutation() {
    // The asymmetry closed only the view spelling: an unannotated parameter is
    // still by value, and mutating it still steers to `mut` or `&mut`.
    assert_fails_with(
        r#"
        fun apply(seed: List<i32>, mutate: sync |List<i32>| void) { mutate(seed); }
        fun main() {
            apply([1, 2], |list| { list.push(5); });
        }
        "#,
        "cannot mutate immutable 'list'",
    );
}

#[test]
fn a_closure_parameter_refuses_mut_combined_with_a_convention() {
    // `mut` and a convention stay non-composable in closure position too
    // (proposal/mut-parameters.md §2), now that both are spellable there.
    assert_fails_with(
        r#"
        fun apply(target: &mut List<i32>, mutate: sync |&mut List<i32>| void) {
            mutate(target);
        }
        fun main() {
            mut data = [1];
            apply(&mut data, |&mut mut list| { list.push(5); });
        }
        "#,
        "it cannot combine with `own` or a view",
    );
}

#[test]
fn a_scalar_shared_write_passes_as_a_mut_view() {
    // The pre-existing bug `update`'s scalar leg exposed, pinned on its own
    // terms — no `Signal` involved. `Shared::write()` over a scalar handed the
    // callee the VALUE, and `slot[0][slot[1]]` on a number crashed at runtime.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        fun replace<T>(slot: &mut T, value: T) { slot = value; }
        fun main() {
            let cell: Shared<i32> = Shared::new(1);
            replace(cell.write(), 9);
            print(cell.read());
            let listed: Shared<List<i32>> = Shared::new([1, 2]);
            replace(listed.write(), [3, 4, 5]);
            print(listed.read().len());
        }
        "#,
        "9\n3\n",
    );
}

#[test]
fn a_shared_write_assignment_is_unchanged_for_both_pointees() {
    // The assign-through path keeps taking the `v` slot, so the pair lowering
    // above cannot regress `cell.write() = x` for a scalar or an aggregate.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        fun main() {
            let flag: Shared<bool> = Shared::new(false);
            flag.write() = true;
            print(flag.read());
            let listed: Shared<List<i32>> = Shared::new([1]);
            listed.write().push(2);
            listed.write() = [7, 8, 9];
            print(listed.read().len());
        }
        "#,
        "true\n3\n",
    );
}

#[test]
fn a_sync_void_parameter_refuses_an_async_closure() {
    // B61: the `sync` marker is the whole contract — what the callback returns
    // decides ADAPTATION, not whether the contract binds. `sync || void` used
    // to accept an awaiting closure that the identical `sync || i32` refused.
    assert_fails_with(
        r#"
        import std::time::sleep;
        fun run_now(body: sync || void) { body(); }
        fun main() {
            run_now(|| { sleep(1); });
        }
        "#,
        "requires a synchronous closure (`sync`)",
    );
}

#[test]
fn a_sync_void_parameter_still_takes_a_synchronous_closure() {
    // The other half of B61: the marker refuses awaiting callbacks, not every
    // callback. A void `sync` parameter is the ordinary case.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun run_now(body: sync || void) { body(); }
        fun main() {
            run_now(|| { print(1); });
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_sync_void_parameter_refuses_a_forwarded_async_closure() {
    // B61 reaches the transitive path too: the closure passed to `run_now` is
    // async only for the instance of `forward` whose `f` awaits, which is the
    // per-instance check — and it gated on the same adaptation shape, so a
    // void `sync` parameter escaped it as well. The value-returning twin is
    // `a_forwarded_async_closure_into_a_sync_contract_is_refused`.
    assert_fails_noting(
        r#"
        import std::time::sleep;
        fun run_now(body: sync || void) { body(); }
        fun forward(f: || i32) { run_now(|| { f(); }); }
        fun main() {
            forward(|| { sleep(1); 2 });
        }
        "#,
        "passes an async closure that reaches `body`, which requires a synchronous closure (`sync`)",
        "run_now(|| { f(); })",
        "forwarded into the `sync` parameter `body` here",
    );
}

#[test]
fn signal_update_refuses_an_awaiting_closure() {
    // A18 declared `Signal::update`'s `mutate` parameter `sync` because a view
    // may not be live across an `await` (spec §6.6) — a correct declaration
    // that did not bite until B61. It bites now.
    assert_fails_with(
        r#"
        import std::reactive::Signal;
        import std::time::sleep;
        fun main() {
            let items: Signal<List<i32>> = Signal::new([1]);
            items.update(|&mut list| { sleep(1); list.push(2); });
        }
        "#,
        "requires a synchronous closure (`sync`)",
    );
}

// === RPC foundation: the generic `call` helper (examples/rpc §4.1) ================

#[test]
fn generic_call_over_a_bounded_transport_decodes() {
    // The RPC foundation's `call<T, Tx: Transport>` shape: a generic function that calls a trait
    // method on a bound-generic transport, `await`s it, and decodes the reply as a generic
    // `T: FromJson` — invoked from a *generic* client that passes its own `Tx`-typed field. Pins
    // that this whole generic-through-generic path monomorphizes (the example isn't auto-run).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::result::Result::{ self, Ok, Err };
        import std::promise::Promise;
        trait Wire { fun send(self, msg: str): Promise<str>; }
        struct Echo {}
        impl Echo with Wire {
            fun send(self, msg: str): Promise<str> { async { msg } }   // echoes the request
        }
        [derive(Json)]
        struct Pt { x: i32 }
        fun fetch<T: FromJson, Tx: Wire>(transport: Tx, msg: str): Result<T, str> {
            let reply = await transport.send(msg);
            T::from_json(reply)                           // decode the generic T from the reply
        }
        struct Client<Tx: Wire> { transport: Tx }
        impl Client<type Tx> {
            fun get(self): Result<Pt, str> {
                fetch(self.transport, "{\"x\":42}")        // T=Pt inferred from the return type
            }
        }
        fun main() {
            let c = Client { transport = Echo {} };
            match c.get() {
                Ok(let p) => print(i"x={p.x}"),
                Err(let e) => print(i"err {e}"),
            }
        }
        "#,
        "x=42\n",
    );
}

// === [derive(Wire)] — the data boundary (proposal/transport-rpc.md §3) ============

#[test]
fn wire_derives_the_json_round_trip() {
    // `[derive(Wire)]` reuses the Json round-trip: a Wire struct/enum encodes and decodes,
    // including nested Wire structs, `List<Wire>`, and Wire enums.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };
        [derive(Wire)]
        struct Point { x: i32, y: i32 }
        [derive(Wire)]
        struct Line { from: Point, to: Point, tags: List<str> }
        [derive(Wire)]
        enum Shape { Seg(Line), Empty }
        fun main() {
            let line = Line { from = Point { x = 1, y = 2 }, to = Point { x = 3, y = 4 }, tags = ["a"] };
            match Line::from_json(line.to_json()) {                          // decoding yields a Result (I3)
                Ok(let back) => {
                    print(i"{back.from.x} {back.from.y} {back.to.x} {back.to.y}");   // 1 2 3 4
                    match Shape::from_json(Shape::Seg(back).to_json()) {
                        Ok(Shape::Seg(let l)) => print(i"seg {l.from.x}"),           // seg 1
                        Ok(Shape::Empty) => print("empty"),
                        Err(let e) => print(e),
                    }
                }
                Err(let e) => print(e),
            }
        }
        "#,
        "1 2 3 4\nseg 1\n",
    );
}

#[test]
fn wire_rejects_a_non_wire_field() {
    // The boundary: a `[derive(Wire)]` type with a non-Wire field (`Password` has no codec)
    // is a compile error — the leak the type system prevents by construction.
    assert_fails(
        r#"
        struct Password { hash: str }
        [derive(Wire)]
        struct User { id: i32, password: Password }
        fun main() {}
        "#,
    );
}

#[test]
fn wire_rejects_a_list_of_non_wire() {
    // The recursive rule: `List<Secret>` is not Wire because `Secret` is not. This pins the
    // Wire check specifically — without it, the conditional `List<T: Json>` impl would let
    // `List<Secret>` slip through the codegen unchecked (the conditional-bound gap).
    assert_fails(
        r#"
        struct Secret { s: str }
        [derive(Wire)]
        struct Bag { items: List<Secret> }
        fun main() {}
        "#,
    );
}

// === [rpc] / [expose] — the service-surface checks (transport-rpc.md §4.2) ========

#[test]
fn rpc_accepts_a_wire_signature() {
    // An `[rpc]` method whose whole signature is Wire compiles: multiple parameters,
    // a container (`List<str>`), a nested `[derive(Wire)]` struct, an `Option` return —
    // and `self` is exempt from the check.
    assert_compiles(
        r#"
        import std::option::Option::{ self, Some, None };
        [derive(Wire)]
        struct Pt { x: i32 }
        struct Service {}
        impl Service {
            [rpc] fun locate(self, id: i32, tags: List<str>, at: Pt): Option<Pt> {
                Some(at)
            }
        }
        fun main() {}
        "#,
    );
}

#[test]
fn rpc_rejects_a_non_wire_parameter() {
    // The exposure rule: an `[rpc]` method cannot take a non-Wire type — the
    // dispatcher would have to decode it off the wire.
    assert_fails(
        r#"
        struct Password { hash: str }
        struct Service {}
        impl Service {
            [rpc] fun store(self, secret: Password) {}
        }
        fun main() {}
        "#,
    );
}

#[test]
fn rpc_rejects_a_non_wire_return() {
    // ...nor return one — the reply crosses the wire.
    assert_fails(
        r#"
        struct Password { hash: str }
        struct Service {}
        impl Service {
            [rpc] fun leak(self): Password {
                Password { hash = "x" }
            }
        }
        fun main() {}
        "#,
    );
}

#[test]
fn expose_accepts_a_signal_of_wire() {
    // An `[expose]`d field must be a `Signal` of a Wire element — a scalar and a
    // `[derive(Wire)]` struct both qualify.
    assert_compiles(
        r#"
        import std::reactive::Signal;
        [derive(Wire)]
        struct Pt { x: i32 }
        struct Session {
            [expose] status: Signal<str>,
            [expose] cursor: Signal<Pt>,
            hidden: i32,
        }
        fun main() {}
        "#,
    );
}

#[test]
fn expose_rejects_a_non_signal_field() {
    // Exposure is observation: a plain value has nothing to subscribe to.
    assert_fails(
        r#"
        struct Session {
            [expose] name: str,
        }
        fun main() {}
        "#,
    );
}

#[test]
fn expose_rejects_a_signal_of_non_wire() {
    // The observed values cross the wire, so the element must be Wire.
    assert_fails(
        r#"
        import std::reactive::Signal;
        struct Password { hash: str }
        struct Session {
            [expose] secret: Signal<Password>,
        }
        fun main() {}
        "#,
    );
}

// === [trait_only] / [doc(hidden)] — namespace hygiene (transport-rpc.md §3.2) =====

#[test]
fn trait_only_method_is_hidden_from_the_concrete_type() {
    // A `[trait_only]` trait method never resolves on the concrete type's own
    // surface — the direct call is an error even though the impl provides it.
    assert_fails(
        r#"
        import std::io::print;
        trait Marker { [trait_only] fun tag(self): str; }
        struct Pt { x: i32 }
        impl Pt with Marker { fun tag(self): str { "pt" } }
        fun main() { print(Pt { x = 1 }.tag()); }
        "#,
    );
}

#[test]
fn trait_only_method_resolves_through_a_bound() {
    // ...but through a trait bound it resolves and monomorphizes normally.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Marker { [trait_only] fun tag(self): str; }
        struct Pt { x: i32 }
        impl Pt with Marker { fun tag(self): str { "pt" } }
        fun describe<T: Marker>(value: T): str { value.tag() }
        fun main() { print(describe(Pt { x = 1 })); }
        "#,
        "pt\n",
    );
}

#[test]
fn trait_only_static_is_hidden_from_the_concrete_type() {
    // The same exclusion covers statics: `Pt::make()` is an error when `make`
    // is `[trait_only]` — the `from_json`-style surface stays clean.
    assert_fails(
        r#"
        trait Factory { [trait_only] fun make(): i32; }
        struct Pt {}
        impl Pt with Factory { fun make(): i32 { 7 } }
        fun main() { let n = Pt::make(); }
        "#,
    );
}

#[test]
fn trait_only_static_resolves_through_a_bound() {
    // ...while `T::make()` through the bound stays the sanctioned path.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Factory { [trait_only] fun make(): i32; }
        struct Pt {}
        impl Pt with Factory { fun make(): i32 { 7 } }
        fun build<T: Factory>(witness: T): i32 { T::make() }
        fun main() { print(build(Pt {})); }
        "#,
        "7\n",
    );
}

#[test]
fn trait_only_default_method_is_bound_reachable_but_hidden() {
    // A `[trait_only]` *default* method: an empty impl inherits it for the
    // bound path, but it is not promoted onto the concrete surface.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Marker { [trait_only] fun tag(self): str { "default" } }
        struct Pt { x: i32 }
        impl Pt with Marker {}
        fun via_bound<T: Marker>(value: T): str { value.tag() }
        fun main() { print(via_bound(Pt { x = 1 })); }
        "#,
        "default\n",
    );
    assert_fails(
        r#"
        import std::io::print;
        trait Marker { [trait_only] fun tag(self): str { "default" } }
        struct Pt { x: i32 }
        impl Pt with Marker {}
        fun main() { print(Pt { x = 1 }.tag()); }
        "#,
    );
}

#[test]
fn trait_only_does_not_shadow_an_inherent_method() {
    // The collision-safety point: a type's OWN method with the same name stays
    // reachable on the concrete surface — the `[trait_only]` trait method never
    // shadows it (nor is shadowed by it at the bound).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Marker { [trait_only] fun tag(self): str { "trait-default" } }
        struct Pt { x: i32 }
        impl Pt { fun tag(self): str { "own" } }
        impl Pt with Marker {}
        fun main() { print(Pt { x = 1 }.tag()); }
        "#,
        "own\n",
    );
}

#[test]
fn bound_dispatch_prefers_the_trait_method_on_a_name_collision() {
    // FIXED: the analyzer resolved `value.tag()` through the `Marker` bound,
    // but the transformer's name-based re-dispatch found the concrete type's
    // INHERENT `tag` first. The resolved trait is now recorded per call
    // (bound_dispatch_traits) and emission dispatches on that trait's surface
    // — override, else default — so an inherent name collision can't shadow it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Marker { fun tag(self): str { "trait-default" } }
        struct Pt { x: i32 }
        impl Pt { fun tag(self): str { "own" } }
        impl Pt with Marker {}
        fun via_bound<T: Marker>(value: T): str { value.tag() }
        fun main() { print(via_bound(Pt { x = 1 })); }
        "#,
        "trait-default\n",
    );
}

// === [service(Client)] generation (transport-rpc.md §4.2) =========================

#[test]
fn service_generates_dispatcher_client_and_mirror() {
    // The whole §4.2 surface, end to end and in-process: `[service(Client)]` generates
    // `Session::dispatcher(self)` (routes both `[rpc]` methods — multi-arg and no-arg),
    // the sibling `Client<T: Transport>` with `Result`-wrapped requestors, and a
    // `RemoteSource` mirror for the `[expose]`d field (whose update arrives in the same
    // wire turn as the mutating call's reply — hence `status = bumped` before `bump -> 5`).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::reactive::Signal;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, FromJson };
        import std::json::json_codec;
        import std::rpc::{ local_rpc, duplex_pair, ReactiveServer, ReactiveClient, RemoteSource };

        [service(Client)]
        struct Session {
            [expose] status: Signal<str>,
            count: Shared<i32>,
        }

        impl Session {
            [rpc]
            fun bump(self, by: i32): i32 {
                self.count.write() = self.count.read() + by;
                self.status.set("bumped");
                self.count.read()
            }

            [rpc]
            fun total(self): i32 {
                self.count.read()
            }
        }

        fun main() {
            let session = Session { status = Signal::new("idle"), count = Shared::new(0) };
            let transport = local_rpc(session.dispatcher().into_protocol(json_codec()));
            let (client_end, server_end) = duplex_pair();
            let channel = ReactiveServer::new(server_end, json_codec()).expose(session.status);
            let mirror: RemoteSource<str> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let client = Client { transport, codec = json_codec(), status = mirror };
            let watching = client.status.sub(|s| {
                print(i"status = {s}");
            });
            match client.bump(5) {
                Ok(let n) => print(i"bump -> {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
            match client.total() {
                Ok(let n) => print(i"total -> {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
            let hashes_match = session.contract_hash() == client.contract_hash();
            print(i"hashes match = {hashes_match}");
            watching.dispose();
        }
        "#,
        "status = idle\nstatus = bumped\nbump -> 5\ntotal -> 5\nhashes match = true\n",
    );
}

#[test]
fn service_client_name_defaults_to_struct_client() {
    // Bare `[service]` names the generated client `<Struct>Client`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, json_codec };
        import std::rpc::{ local_rpc };

        [service]
        struct Counter {
            count: Shared<i32>,
        }

        impl Counter {
            [rpc]
            fun get(self): i32 {
                self.count.read()
            }
        }

        fun main() {
            let counter = Counter { count = Shared::new(41) };
            let transport = local_rpc(counter.dispatcher().into_protocol(json_codec()));
            let client = CounterClient { transport, codec = json_codec() };
            match client.get() {
                Ok(let n) => print(i"n = {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
        }
        "#,
        "n = 41\n",
    );
}

#[test]
fn service_contract_verify_matches_and_catches_drift() {
    // The generated `verify()` (Q6 v2): a client fetches the server's contract hash
    // over the built-in `__contract` route and compares. Against its own service:
    // `Ok(true)`. Against a *different* service's dispatcher (a drifted contract —
    // the versioning failure mode): `Ok(false)`, a clean signal instead of decode
    // garbage.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, json_codec };
        import std::rpc::{ local_rpc };

        [service(AClient)]
        struct Alpha { count: Shared<i32> }
        impl Alpha {
            [rpc] fun ping(self): i32 { 1 }
        }

        [service(BClient)]
        struct Beta { count: Shared<i32> }
        impl Beta {
            [rpc] fun rename(self, name: str): str { name }
        }

        fun main() {
            let alpha_transport = local_rpc(Alpha { count = Shared::new(0) }.dispatcher().into_protocol(json_codec()));
            let matching = AClient { transport = alpha_transport, codec = json_codec() };
            match matching.verify() {
                Ok(let same) => print(i"self = {same}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
            // A BClient pointed at Alpha's dispatcher — the drift case.
            let drifted = BClient { transport = alpha_transport, codec = json_codec() };
            match drifted.verify() {
                Ok(let same) => print(i"drift = {same}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
        }
        "#,
        "self = true\ndrift = false\n",
    );
}

// === Async rpc handlers (the dispatch spine awaits — J2 through the wire) =========

#[test]
fn an_async_rpc_method_replies_after_its_await() {
    // The user-shaped case: a `[rpc]` method that awaits (here `sleep_for`)
    // compiles, and its reply carries the value computed AFTER the suspension.
    // The `[service]` macro wraps each route in a held `turn`, and every seam
    // of the spine (`Dispatcher.handle` → `RpcProtocol.respond` →
    // `LocalTransport.call`) awaits through a re-marked `let` (J2 v1).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, json_codec };
        import std::rpc::{ local_rpc };
        import std::time::{ sleep_for, Duration };

        [service(SlowClient)]
        struct Slow { calls: Shared<i32> }

        impl Slow {
            [rpc]
            fun slow_double(self, by: i32): i32 {
                self.calls.write() = self.calls.read() + 1;
                sleep_for(Duration::millis(10));
                by * 2
            }
        }

        fun main() {
            let service = Slow { calls = Shared::new(0) };
            let transport = local_rpc(service.dispatcher().into_protocol(json_codec()));
            let client = SlowClient { transport, codec = json_codec() };
            match client.slow_double(7) {
                Ok(let n) => print(i"slow_double -> {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
            print(i"calls = {service.calls.read()}");
        }
        "#,
        "slow_double -> 14\ncalls = 1\n",
    );
}

#[test]
fn sync_and_async_rpc_methods_coexist_on_one_service() {
    // J2 in both directions through the retyped spine: the sync method rides
    // the same `async |..|`-seamed dispatch (awaiting a plain value just
    // resolves), the async one settles before its reply encodes.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, json_codec };
        import std::rpc::{ local_rpc };
        import std::time::{ sleep_for, Duration };

        [service(MixedClient)]
        struct Mixed { count: Shared<i32> }

        impl Mixed {
            [rpc]
            fun quick(self): i32 { 1 }

            [rpc]
            fun slow(self): i32 {
                sleep_for(Duration::millis(5));
                2
            }
        }

        fun main() {
            let transport = local_rpc(
                Mixed { count = Shared::new(0) }.dispatcher().into_protocol(json_codec()),
            );
            let client = MixedClient { transport, codec = json_codec() };
            match client.quick() {
                Ok(let n) => print(i"quick -> {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
            match client.slow() {
                Ok(let n) => print(i"slow -> {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
        }
        "#,
        "quick -> 1\nslow -> 2\n",
    );
}

#[test]
fn an_async_rpc_methods_writes_settle_as_one_wave_with_its_reply() {
    // The wire turn HOLDS across the handler's await (an awaiting `turn` body, the true
    // at-end cadence): a write before and a write after the suspension
    // coalesce, so the mirror sees ONE update — the final value — alongside
    // the reply. (Per-segment settling would leak "working" as its own
    // update before the reply.)
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::reactive::Signal;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, FromJson };
        import std::json::json_codec;
        import std::rpc::{ local_rpc, duplex_pair, ReactiveServer, ReactiveClient, RemoteSource };
        import std::time::{ sleep_for, Duration };

        [service(JobClient)]
        struct Job {
            [expose] status: Signal<str>,
        }

        impl Job {
            [rpc]
            fun run(self): i32 {
                self.status.set("working");
                sleep_for(Duration::millis(10));
                self.status.set("done");
                7
            }
        }

        fun main() {
            let job = Job { status = Signal::new("idle") };
            let transport = local_rpc(job.dispatcher().into_protocol(json_codec()));
            let (client_end, server_end) = duplex_pair();
            let channel = ReactiveServer::new(server_end, json_codec()).expose(job.status);
            let mirror: RemoteSource<str> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let client = JobClient { transport, codec = json_codec(), status = mirror };
            let watching = client.status.sub(|s| {
                print(i"status = {s}");
            });
            match client.run() {
                Ok(let n) => print(i"run -> {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
            watching.dispose();
        }
        "#,
        "status = idle\nstatus = done\nrun -> 7\n",
    );
}

#[test]
fn a_no_arg_rpc_methods_writes_coalesce_in_the_wire_turn() {
    // The hole the wave pin uncovered, pinned on its own (no async involved):
    // no-arg methods once took a bare `.on(..)` fast path that skipped the
    // wire turn entirely, so each write leaked as its own update. Every
    // method route now goes through `route_block`'s turn — two writes in a
    // sync no-arg method arrive at the mirror as ONE update, the final value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::Signal;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, FromJson };
        import std::json::json_codec;
        import std::rpc::{ local_rpc, duplex_pair, ReactiveServer, ReactiveClient, RemoteSource };

        [service(FlipClient)]
        struct Flip {
            [expose] state: Signal<str>,
        }

        impl Flip {
            [rpc]
            fun flip(self): i32 {
                self.state.set("mid");
                self.state.set("final");
                1
            }
        }

        fun main() {
            let flip = Flip { state = Signal::new("start") };
            let transport = local_rpc(flip.dispatcher().into_protocol(json_codec()));
            let (client_end, server_end) = duplex_pair();
            let channel = ReactiveServer::new(server_end, json_codec()).expose(flip.state);
            let mirror: RemoteSource<str> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let client = FlipClient { transport, codec = json_codec(), state = mirror };
            let watching = client.state.sub(|s| {
                print(i"state = {s}");
            });
            match client.flip() {
                Ok(let n) => print(i"flip -> {n}"),
                Err(let error) => print(i"err {error.to_json()}"),
            }
            watching.dispose();
        }
        "#,
        "state = start\nstate = final\nflip -> 1\n",
    );
}

#[test]
fn a_hand_written_async_route_dispatches_through_respond() {
    // The foundation API without the macro: an async handler registered with
    // `Dispatcher.on` (its `async |..|` parameter), driven through `respond`
    // directly — the reply envelope encodes the settled outcome.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::json_codec;
        import std::wire::Frame;
        import std::rpc::{ Dispatcher, reply, encode_request, RpcOutcome };
        import std::time::{ sleep_for, Duration };

        fun main() {
            let protocol = Dispatcher::new()
                .on("slow", |request| {
                    sleep_for(Duration::millis(5));
                    reply(21)
                })
                .into_protocol(json_codec());
            let answer = protocol.respond(encode_request(json_codec(), "slow", []));
            match answer {
                Frame::Text(let envelope) => print(i"answer: {envelope}"),
                Frame::Binary(let bytes) => print("answer: unexpected binary"),
            }
        }
        "#,
        "answer: {\"Success\":21}\n",
    );
}

#[test]
fn rpc_rejects_a_missing_return() {
    // A void `[rpc]` method has no reply payload to encode — the return must be a
    // declared Wire type (fire-and-forget needs its own design).
    assert_fails(
        r#"
        struct Service {}
        impl Service {
            [rpc] fun ping(self) {}
        }
        fun main() {}
        "#,
    );
}

#[test]
fn a_discarded_async_block_still_runs() {
    // `async { .. }` is an *invoked* async arrow: its body starts executing
    // immediately (up to the first await), so it is effectful even when the
    // promise is discarded. The transformer's side-effect analysis used to
    // classify it as pure and elide the whole statement — `let _ = async { pump
    // loop }` silently vanished from codegen (found via SplitDuplex's pump).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let _ = async {
                print("ran");
            };
            print("after");
        }
        "#,
        "ran\nafter\n",
    );
}

#[test]
fn a_parenthesized_type_is_grouping_not_a_tuple() {
    // `(T)` in type position is grouping, not a one-tuple — required to write a
    // closure-typed closure parameter (`|(|| void)| void`, the host-Promise
    // executor shape `std::time::sleep` uses). The inner closure is passed AND
    // called through the parenthesized annotation.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun run_with(callback: |(|| void)| void) {
            callback(|| print("called"));
        }
        fun main() {
            run_with(|done: || void| {
                done();
            });
        }
        "#,
        "called\n",
    );
}

#[test]
fn calling_an_unannotated_closure_parameter_defers() {
    // FIXED: a free call whose SUBJECT is an unannotated closure parameter
    // (`|done| { done(); }`) now defers until bidirectional inference lands
    // the parameter's type — the same rule the method-receiver and argument
    // paths already had (Bug C′'s family).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun run_with(callback: |(|| void)| void) {
            callback(|| print("called"));
        }
        fun main() {
            run_with(|done| {
                done();
            });
        }
        "#,
        "called\n",
    );
}

#[test]
fn doc_hidden_method_stays_callable() {
    // `[doc(hidden)]` is tooling-only: completion omits it, resolution doesn't.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct Pt { x: i32 }
        impl Pt {
            [doc(hidden)]
            fun secret(self): i32 { self.x }
        }
        fun main() { print(Pt { x = 9 }.secret()); }
        "#,
        "9\n",
    );
}

#[test]
fn emitted_js_preserves_grouping_across_precedence() {
    // A latent emitter miscompile (found by the bits-and-bytes probe,
    // proposal/bits-and-bytes.md §0): the JS printer rendered binary operands
    // flat, so `(1 + 2) * 3` emitted as `1 + 2 * 3` and printed 7. Operands are
    // now parenthesized by JS precedence.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print((1 + 2) * 3);
            let a = 1;
            let b = 2;
            let c = 3;
            print((a + b) * c);
            print(0 - (a - b));
            print(a - (b - c));
            print((a + b) / (b + c) + 1);
            print((1.0 + 2.0) / (2.0 + 3.0) + 1.0);
        }
        "#,
        "9\n9\n1\n2\n1\n1.6\n",
    );
}

#[test]
fn emitted_js_parenthesizes_right_nested_addition() {
    // `+` is left-associative but not insensitive to grouping: float addition
    // does not reassociate, so `0.1 + (0.2 + 0.3)` is 0.6 where the flat
    // `0.1 + 0.2 + 0.3` is 0.6000000000000001. The printer must keep the
    // parentheses on a right-nested operand of equal precedence.
    //
    // This case was written as `1 + (2 + "x")` until B148: an `i32 + str`,
    // which the native path's operand rule now refuses. It was itself an
    // instance of the bug — the expression took its type from the LEFT operand
    // and so type-checked as `i32` while the host produced the string "12x".
    // The printer property is unchanged; only the operands had to become ones
    // the language admits.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let start = 0.1;
            print(start + (0.2 + 0.3));
        }
        "#,
        "0.6\n",
    );
}

#[test]
fn hex_literals_type_and_evaluate_like_decimal() {
    // `0x` is a spelling, not a type: suffix, context, and the i32 default all
    // apply, and the literal reaches JS verbatim (proposal/bits-and-bytes.md §1).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(0xFF);
            print(0x10 + 1);
            let big = 0xDEADn;
            print(big);
            print(i"masked = {0xF0 & 0x1F}");
        }
        "#,
        "255\n17\n57005n\nmasked = 16\n",
    );
}

#[test]
fn bitwise_operators_on_i32_use_signed_js_semantics() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(12 & 10);
            print(12 | 3);
            print(12 ^ 10);
            print(1 << 5);
            print(0 - 8 >> 1);
        }
        "#,
        "8\n15\n6\n32\n-4\n",
    );
}

#[test]
fn bitwise_operators_on_u32_stay_unsigned() {
    // JS bitwise is signed; `u32` results re-wrap with `>>> 0` and `>>` is the
    // logical `>>>` — a set high bit must come back as a large unsigned value
    // (proposal/bits-and-bytes.md §2).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let high: u32 = 0x80000000;
            print(high | 0);
            print(high >> 4);
            print(0xFFFFFFFFu32 >> 28);
            let one: u32 = 1;
            print(one << 31);
            print(0xF0F0F0F0u32 ^ 0xFFFFFFFFu32);
        }
        "#,
        "2147483648\n134217728\n15\n2147483648\n252645135\n",
    );
}

#[test]
fn bitwise_operators_on_bigint_do_not_wrap() {
    // BigInt is arbitrary-precision: the native JS operators apply and the u32
    // `>>> 0` normalization must NOT — `1n << 64n` exceeds 64 bits intact.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(0xFFn & 0x0Fn);
            print(1n << 64n);
        }
        "#,
        "15n\n18446744073709551616n\n",
    );
}

#[test]
fn bitwise_precedence_is_rust_order_not_c_order() {
    // `<< >>` over `&` over `^` over `|`, all over comparisons — so
    // `1 << 2 == 4` is `(1 << 2) == 4` and `1 | 2 ^ 2 & 3` is `1 | (2 ^ (2 & 3))`.
    // Emission must survive JS's DIFFERENT (C-style) order via parentheses.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(1 << 2 == 4);
            print(1 | 2 ^ 2 & 3);
            print((1 | 2) & 3 == 3);
            let masked = 0xFF & 0x0F;
            print(masked == 15);
        }
        "#,
        "true\n1\ntrue\ntrue\n",
    );
}

#[test]
fn shifts_coexist_with_nested_generics() {
    // `<<`/`>>` are two ADJACENT control tokens in expression position;
    // `List<List<i32>>` (type position) and comparisons are untouched.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let nested: List<List<i32>> = [[1, 2], [3]];
            let shifted = nested.len() << 2;
            print(shifted);
            print(1 < 2);
        }
        "#,
        "8\ntrue\n",
    );
}

#[test]
fn split_shift_stays_a_parse_error() {
    // Adjacency is load-bearing: `a < < b` must not silently become a shift.
    assert_fails(
        r#"
        fun main() {
            let a = 1;
            let b = 2;
            let c = a < < b;
        }
        "#,
    );
}

#[test]
fn bitand_dispatches_to_the_operator_trait() {
    // `&` on a struct routes through `std::operators::BitAnd::bit_and`,
    // mirroring `+`/`Add`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::BitAnd;
        struct Flags { bits: i32 }
        impl Flags with BitAnd {
            fun bit_and(self, other: Flags): Flags {
                Flags { bits = self.bits & other.bits }
            }
        }
        fun main() {
            let a = Flags { bits = 12 };
            let b = Flags { bits = 10 };
            print((a & b).bits);
        }
        "#,
        "8\n",
    );
}

#[test]
fn missing_bitwise_impl_names_the_trait() {
    // A non-native type without the impl gets the operator diagnostic naming
    // the trait, mirroring `Add`.
    assert_fails(
        r#"
        struct Flags { bits: i32 }
        fun main() {
            let a = Flags { bits = 1 };
            let b = Flags { bits = 2 };
            let c = a ^ b;
        }
        "#,
    );
}

#[test]
fn bytes_buffers_round_trip() {
    // `std::bytes` (proposal/bits-and-bytes.md §3): alloc/len/get/set with the
    // host's `& 0xFF` store semantics, slice, concat, and a multibyte UTF-8
    // round-trip. The codec substrate.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::bytes::{ Bytes, encode_utf8, decode_utf8 };
        fun main() {
            let buffer = Bytes::alloc(4);
            print(buffer.len());
            buffer.set(0, 0xDE);
            buffer.set(1, 0x1FF);
            print(buffer.get(0));
            print(buffer.get(1));
            print(buffer.get(2));
            let joined = Bytes::concat(buffer.slice(0, 2), buffer);
            print(joined.len());
            let text = "héllo 🎉";
            let encoded = encode_utf8(text);
            print(encoded.len());
            print(decode_utf8(encoded) == text);
        }
        "#,
        "4\n222\n255\n0\n6\n11\ntrue\n",
    );
}

#[test]
fn generic_trait_method_dispatches_through_a_bound() {
    // FIXED: a trait method with its OWN generic parameters (describe<S: Sink>)
    // used to no-op silently through `T: Describable` — the OnConstraint
    // emission re-targeted the concrete impl's method without the call's
    // own-generic bindings (whose ids belong to the TRAIT member), so the
    // instance emitted with S unbound. The bindings now cross the re-dispatch
    // as ordered values, zipped onto the target's own generics.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        trait Sink { fun put(self, value: i32); }
        struct Collector { total: Shared<i32> }
        impl Collector with Sink {
            fun put(self, value: i32) {
                self.total.write() = self.total.read() + value;
            }
        }
        trait Describable {
            fun describe<S: Sink>(self, sink: S);
        }
        struct Point { x: i32, y: i32 }
        impl Point with Describable {
            fun describe<S: Sink>(self, sink: S) {
                sink.put(self.x);
                sink.put(self.y);
            }
        }
        fun encode<T: Describable, S: Sink>(value: T, sink: S) {
            value.describe(sink);
        }
        fun main() {
            let collector = Collector { total = Shared::new(0) };
            let point = Point { x = 3, y = 4 };
            point.describe(collector);
            print(collector.total.read());
            encode(point, collector);
            print(collector.total.read());
        }
        "#,
        "7\n14\n",
    );
}

#[test]
fn impl_binder_in_trait_argument_position() {
    // One impl serving every sink: the binder sits in the TRAIT argument,
    // registered like a subject binder (bound-less ones inherit the trait's
    // declared bound for the position) — transport-rpc.md §6.1's other gap,
    // closed.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        trait Sink { fun put(self, value: i32); }
        struct Collector { total: Shared<i32> }
        impl Collector with Sink {
            fun put(self, value: i32) {
                self.total.write() = self.total.read() + value;
            }
        }
        trait DescribeInto<S> {
            fun describe_into(self, sink: S);
        }
        struct Point { x: i32 }
        impl Point with DescribeInto<type S: Sink> {
            fun describe_into(self, sink: S) {
                sink.put(self.x);
            }
        }
        fun main() {
            let point = Point { x = 3 };
            let collector = Collector { total = Shared::new(0) };
            point.describe_into(collector);
            print(collector.total.read());
        }
        "#,
        "3\n",
    );
}

#[test]
fn hand_written_wire_impls_round_trip_through_json() {
    // The §6.1 visitor, proven hand-written before the derive targets it: a
    // struct (scalar/list/option/nested-enum fields) and an enum (0/1/2-arity
    // variants) describe to `JsonWriter` and rebuild from `JsonReader`. The
    // encoded text must match the established `to_json` wire format exactly
    // (externally-tagged variants, arity>1 payload arrays, bare `Some`,
    // `null` for `None`), and structural failures are sticky decode errors —
    // backlog I3's validating decode.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::wire::{ Wire, Serialize, Deserialize };
        import std::json::{ encode_json, decode_json };

        enum Status {
            Offline,
            Away(str),
            Busy(str, i32),
        }

        impl Status with Wire {
            fun describe<S: Serialize>(self, serializer: S) {
                match self {
                    Status::Offline => {
                        serializer.begin_variant("Offline", 0);
                        serializer.end_variant();
                    },
                    Status::Away(let reason) => {
                        serializer.begin_variant("Away", 1);
                        reason.describe(serializer);
                        serializer.end_variant();
                    },
                    Status::Busy(let task, let minutes) => {
                        serializer.begin_variant("Busy", 2);
                        task.describe(serializer);
                        minutes.describe(serializer);
                        serializer.end_variant();
                    },
                }
            }

            fun rebuild<D: Deserialize>(deserializer: D): Status {
                let tag = deserializer.variant_tag();
                match tag {
                    "Offline" => {
                        deserializer.begin_variant("Offline", 0);
                        deserializer.end_variant();
                        Status::Offline
                    },
                    "Away" => {
                        deserializer.begin_variant("Away", 1);
                        let reason = str::rebuild(deserializer);
                        deserializer.end_variant();
                        Status::Away(reason)
                    },
                    "Busy" => {
                        deserializer.begin_variant("Busy", 2);
                        let task = str::rebuild(deserializer);
                        let minutes = i32::rebuild(deserializer);
                        deserializer.end_variant();
                        Status::Busy(task, minutes)
                    },
                    _ => {
                        deserializer.fail(i"unknown variant '{tag}'");
                        Status::Offline
                    },
                }
            }
        }

        struct Profile {
            id: i32,
            name: str,
            scores: List<i32>,
            nickname: Option<str>,
            status: Status,
        }

        impl Profile with Wire {
            fun describe<S: Serialize>(self, serializer: S) {
                serializer.begin_struct(5);
                serializer.field("id");
                self.id.describe(serializer);
                serializer.field("name");
                self.name.describe(serializer);
                serializer.field("scores");
                self.scores.describe(serializer);
                serializer.field("nickname");
                self.nickname.describe(serializer);
                serializer.field("status");
                self.status.describe(serializer);
                serializer.end_struct();
            }

            fun rebuild<D: Deserialize>(deserializer: D): Profile {
                deserializer.begin_struct();
                deserializer.field("id");
                let id = i32::rebuild(deserializer);
                deserializer.field("name");
                let name = str::rebuild(deserializer);
                deserializer.field("scores");
                let scores: List<i32> = List::rebuild(deserializer);
                deserializer.field("nickname");
                let nickname: Option<str> = Option::rebuild(deserializer);
                deserializer.field("status");
                let status = Status::rebuild(deserializer);
                deserializer.end_struct();
                Profile { id = id, name = name, scores = scores, nickname = nickname, status = status }
            }
        }

        fun main() {
            let profile = Profile {
                id = 7,
                name = "ada \"the\" first",
                scores = [3, 1, 4],
                nickname = None,
                status = Status::Busy("proofs", 45),
            };
            let encoded = encode_json(profile);
            print(encoded);
            let decoded: Result<Profile, str> = decode_json(encoded);
            match decoded {
                Ok(let back) => {
                    print(back.id);
                    print(back.scores.len());
                    match back.status {
                        Status::Busy(let task, let minutes) => print(i"busy {task} {minutes}"),
                        _ => print("wrong status"),
                    }
                },
                Err(let reason) => print(i"decode failed: {reason}"),
            }
            print(encode_json(Profile { id = 1, name = "bob", scores = [], nickname = Some("bo"), status = Status::Away("lunch") }));
            let missing: Result<Profile, str> = decode_json("{\"id\":1,\"name\":\"x\",\"scores\":[]}");
            match missing {
                Ok(let value) => print("should have failed"),
                Err(let reason) => print(i"err: {reason}"),
            }
            let unknown: Result<Status, str> = decode_json("{\"Vanished\":1}");
            match unknown {
                Ok(let value) => print("should have failed"),
                Err(let reason) => print(i"err: {reason}"),
            }
        }
        "#,
        "{\"id\":7,\"name\":\"ada \\\"the\\\" first\",\"scores\":[3,1,4],\"nickname\":null,\"status\":{\"Busy\":[\"proofs\",45]}}\n7\n3\nbusy proofs 45\n{\"id\":1,\"name\":\"bob\",\"scores\":[],\"nickname\":\"bo\",\"status\":{\"Away\":\"lunch\"}}\nerr: missing field 'nickname'\nerr: unknown variant 'Vanished'\n",
    );
}

#[test]
fn qualified_generic_static_resolves_inner_trait_statics() {
    // FIXED: `List<i32>::rebuild(d)` (the qualified-generic spelling) used to
    // emit the inner `T::rebuild` as an EMPTY function — the accessor resolution
    // discarded the subject's type args entirely. A qualified subject now seeds
    // the matched impl's binder bindings into ITS call's substitution.
    //
    // `build` returns `Self`, not `Build`: it is the `wire.vl` shape, and this
    // program was written in the same stand-in style std was, before B4 §11
    // gave those declarations their real spelling.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Build {
            fun build(seed: i32): Self;
        }
        impl i32 with Build {
            fun build(seed: i32): i32 { seed + 1 }
        }
        struct Boxy<T> { value: T }
        impl Boxy<type T: Build> {
            fun make(seed: i32): Boxy<T> {
                Boxy { value = T::build(seed) }
            }
        }
        fun main() {
            let via_annotation: Boxy<i32> = Boxy::make(1);
            print(via_annotation.value);
            let via_qualified = Boxy<i32>::make(1);
            print(via_qualified.value);
        }
        "#,
        "2\n2\n",
    );
}

#[test]
fn derived_wire_visitor_matches_to_json_and_round_trips() {
    // `[derive(Wire)]` now also emits the §6.1 visitor impls: the described
    // output must equal the derived `to_json` byte-for-byte, rebuild must
    // round-trip (scalars, List, Option, a nested derived enum), and
    // structural failures surface as sticky decode errors through the
    // GENERATED rebuilds.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, encode_json, decode_json };

        [derive(Wire)]
        enum Status {
            Offline,
            Away(str),
            Busy(str, i32),
        }

        [derive(Wire)]
        struct Profile {
            id: i32,
            name: str,
            scores: List<i32>,
            nickname: Option<str>,
            status: Status,
        }

        fun main() {
            let profile = Profile {
                id = 7,
                name = "ada",
                scores = [3, 1, 4],
                nickname = None,
                status = Status::Busy("proofs", 45),
            };
            let via_visitor = encode_json(profile);
            print(via_visitor == profile.to_json());
            let decoded: Result<Profile, str> = decode_json(via_visitor);
            match decoded {
                Ok(let back) => {
                    print(back.id);
                    match back.status {
                        Status::Busy(let task, let minutes) => print(i"busy {task} {minutes}"),
                        _ => print("wrong"),
                    }
                },
                Err(let reason) => print(i"failed: {reason}"),
            }
            let missing: Result<Profile, str> = decode_json("{\"id\":1}");
            match missing {
                Ok(let value) => print("should fail"),
                Err(let reason) => print(i"err: {reason}"),
            }
            let unknown: Result<Status, str> = decode_json("\"Vanished\"");
            match unknown {
                Ok(let value) => print("should fail"),
                Err(let reason) => print(i"err: {reason}"),
            }
        }
        "#,
        "true\n7\nbusy proofs 45\nerr: missing field 'name'\nerr: unknown variant 'Vanished'\n",
    );
}

#[test]
fn derived_struct_with_two_differently_typed_options() {
    // FIXED (same root as the qualified-static gap): with the subject's type
    // args discarded, `Option<str>::from_json_value(..)` and
    // `Option<i32>::from_json_value(..)` in one generated function fought over
    // one shared binder — use sites failed with "Expected Option<i32>, but got
    // Option<str>". Per-call subject bindings keep the two instantiations apart.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        [derive(Json)]
        struct OnlyOptions {
            nick: Option<str>,
            zero: Option<i32>,
        }
        fun main() {
            let value = OnlyOptions { nick = Some("bo"), zero = Some(0) };
            match value.nick {
                Some(let nick) => print(i"nick {nick}"),
                None => print("no nick"),
            }
            match value.zero {
                Some(let zero) => print(i"zero {zero}"),
                None => print("no zero"),
            }
        }
        "#,
        "nick bo\nzero 0\n",
    );
}

#[test]
fn both_codecs_round_trip_derived_wire_values() {
    // §6.2 end-to-end: one derived value through `json_codec()` and
    // `binary_codec()` — negative i32, high-bit u32, f64, multibyte str,
    // List, BOTH Option marker paths (Some(0) is exactly what the binary
    // `0x01` marker disambiguates from None's `0x00`), and a 2-arity enum.
    // Plus the failure modes: a frame of the wrong kind arrives pre-poisoned,
    // and a truncated binary frame fails sticky instead of crashing.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::wire::{ Wire, Frame, Codec, encode, decode };
        import std::json::{ Json, json_codec };
        import std::binary::binary_codec;

        [derive(Wire)]
        enum Status {
            Offline,
            Busy(str, i32),
        }

        [derive(Wire)]
        struct Probe {
            id: i32,
            big: u32,
            ratio: f64,
            label: str,
            flags: List<bool>,
            zero: Option<i32>,
            status: Status,
        }

        fun sample(zero: Option<i32>): Probe {
            Probe {
                id = 0 - 42,
                big = 0xDEADBEEF,
                ratio = 0.5,
                label = "héllo 🎉",
                flags = [true, false, true],
                zero = zero,
                status = Status::Busy("proofs", 45),
            }
        }

        fun check(name: str, back: Result<Probe, str>) {
            match back {
                Ok(let value) => {
                    let intact =
                        value.id == 0 - 42 && value.big == 0xDEADBEEFu32
                        && value.ratio == 0.5 && value.label == "héllo 🎉"
                        && value.flags.len() == 3;
                    print(i"{name} intact = {intact}");
                    match value.zero {
                        Some(let n) => print(i"{name} zero = {n}"),
                        None => print(i"{name} zero = none"),
                    }
                },
                Err(let reason) => print(i"{name} failed: {reason}"),
            }
        }

        fun main() {
            let json = json_codec();
            let binary = binary_codec();
            check("json", decode(json, encode(json, sample(Some(0)))));
            check("binary", decode(binary, encode(binary, sample(Some(0)))));
            check("binary-none", decode(binary, encode(binary, sample(None))));
            let crossed: Result<Probe, str> = decode(binary, encode(json, sample(Some(0))));
            match crossed {
                Ok(let value) => print("should fail"),
                Err(let reason) => print(i"err: {reason}"),
            }
            match encode(binary, sample(Some(0))) {
                Frame::Binary(let whole) => {
                    let cut: Result<Probe, str> = decode(binary, Frame::Binary(whole.slice(0, 9)));
                    match cut {
                        Ok(let value) => print("should fail"),
                        Err(let reason) => print(i"err: {reason}"),
                    }
                },
                Frame::Text(let text) => print("unexpected"),
            }
        }
        "#,
        "json intact = true\njson zero = 0\nbinary intact = true\nbinary zero = 0\nbinary-none intact = true\nbinary-none zero = none\nerr: binary codec: received a text frame\nerr: unexpected end of frame\n",
    );
}

#[test]
fn generated_decode_gate_rejects_a_garbled_request() {
    // The §4.1 validating decode, end to end through GENERATED code: a raw
    // envelope calling `add` with no arguments makes the handler's arg pull
    // fail (binary: out of bounds), and the generated `decode_failed` gate
    // returns `RpcError::Decode` instead of running the impl on zero values —
    // the server's counter must still be 0 afterwards.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::result::Result::{ self, Ok, Err };
        import std::json::Json;
        import std::binary::binary_codec;
        import std::rpc::{ local_rpc, RpcError, call };

        [service(Client)]
        struct Counter {
            count: Shared<i32>,
        }

        impl Counter {
            [rpc]
            fun add(self, by: i32): i32 {
                self.count.write() = self.count.read() + by;
                self.count.read()
            }
        }

        fun main() {
            let counter = Counter { count = Shared::new(0) };
            let transport = local_rpc(counter.dispatcher().into_protocol(binary_codec()));
            // A hand-built envelope with ZERO args for a one-arg method.
            let garbled: Result<i32, RpcError> = call(transport, binary_codec(), "add", []);
            match garbled {
                Ok(let value) => print("should have failed"),
                Err(let error) => print(i"err: {error.to_json()}"),
            }
            let untouched = counter.count.read();
            print(i"count still {untouched}");
        }
        "#,
        "err: {\"Decode\":\"unexpected end of frame\"}\ncount still 0\n",
    );
}

#[test]
fn ws_parser_handles_the_rfc_vectors() {
    // std::ws (transport-rpc.md §5): the RFC 6455 masked "Hello" vector, the
    // same frame split across two feeds, our own encoder round-tripped, the
    // 16-bit length ladder, fragmentation reassembly, ping surfacing, and
    // close ending the stream (later frames ignored).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::bytes::{ Bytes, encode_utf8 };
        import std::ws::{ WsParser, WsEvent, text_frame, encode_frame, close_frame };

        fun show(events: List<WsEvent>) {
            for event in events {
                match event {
                    WsEvent::Text(let text) => print(i"text: {text}"),
                    WsEvent::Binary(let bytes) => print(i"binary: {bytes.len()} bytes"),
                    WsEvent::Ping(let payload) => print(i"ping: {payload.len()} bytes"),
                    WsEvent::Closed => print("closed"),
                }
            }
        }

        fun masked_hello(): Bytes {
            let masked = Bytes::alloc(11);
            masked.set(0, 0x81);
            masked.set(1, 0x85);
            masked.set(2, 0x37);
            masked.set(3, 0xFA);
            masked.set(4, 0x21);
            masked.set(5, 0x3D);
            masked.set(6, 0x7F);
            masked.set(7, 0x9F);
            masked.set(8, 0x4D);
            masked.set(9, 0x51);
            masked.set(10, 0x58);
            masked
        }

        fun main() {
            let parser = WsParser::new();
            show(parser.feed(masked_hello()));
            let splitter = WsParser::new();
            show(splitter.feed(masked_hello().slice(0, 5)));
            print("(partial fed)");
            show(splitter.feed(masked_hello().slice(5, 11)));
            let echo = WsParser::new();
            show(echo.feed(text_frame("server says hi")));
            let big = encode_frame(0x2, Bytes::alloc(200));
            print(i"200B frame = {big.len()} bytes on the wire");
            show(echo.feed(big));
            let part1 = text_frame("Hel");
            part1.set(0, 0x01);
            let part2 = text_frame("lo");
            part2.set(0, 0x80);
            let fragmented = WsParser::new();
            show(fragmented.feed(Bytes::concat(part1, part2)));
            let control = WsParser::new();
            show(control.feed(encode_frame(0x9, encode_utf8("hb"))));
            show(control.feed(close_frame()));
            show(control.feed(text_frame("after close")));
            print("done");
        }
        "#,
        "text: Hello\n(partial fed)\ntext: Hello\ntext: server says hi\n200B frame = 204 bytes on the wire\nbinary: 200 bytes\ntext: Hello\nping: 2 bytes\nclosed\ndone\n",
    );
}

#[test]
fn client_connect_enforces_the_contract_and_wires_mirrors() {
    // §4.2's Client::connect, end to end over a real WebSocket: one generated
    // call opens the socket, VERIFIES the contract hash (Q6 enforcement — the
    // drift case below refuses with Err(Contract) before any decode), calls
    // the generated __attach against the runtime session registry
    // (`Service::new`'s default lifecycle), and wires one RemoteSource mirror
    // per [expose]d field in declaration order — both mirrors deliver.
    //
    // Both servers bind port 0 and the ready callbacks report what they got
    // (backlog E19): literals collided in the v0.12.0 release gate
    // (EADDRINUSE on a re-run), on Windows the 45000-48500 band sits inside
    // the ranges Hyper-V/WSL reserve outright (windows-support.md §4), and a
    // probe-then-substitute port keeps a TOCTOU window the OS can close for us.
    assert_compiles_and_runs(
        &r#"
import std::io::print;
        import std::process::exit;
        import std::time::sleep;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, FromJson, json_codec };
        import std::reactive::Signal;
        import std::shared::Shared;
        import std::rpc_server::Service;
        import std::http::{ Response, Server };

        // The whole paradigm, zero manual wiring: [expose]d state + [rpc] methods,
        // a Service on the server's builder, Client::connect on the client.
        [service(Client)]
        struct Board {
        	[expose] count: Signal<i32>,
        	[expose] label: Signal<str>,
        	total: Shared<i32>,
        }
        
        impl Board {
        	[rpc]
        	fun add(self, by: i32): i32 {
        		self.count.set(self.count.get() + by);
        		self.total.write() = self.total.read() + by;
        		self.label.set(i"sum {self.count.get()}");
        		self.count.get()
        	}
        }
        
        // A second, DIFFERENT service on another port — the drift case.
        [service(OtherClient)]
        struct Other {
        	value: Shared<i32>,
        }
        
        impl Other {
        	[rpc]
        	fun ping(self): i32 { 1 }
        }
        
        fun main() {
        	let board = Board { count = Signal::new(0), label = Signal::new(""), total = Shared::new(0) };
        	Server::builder()
        		.port(0)
        		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
        		.on_request(|request| Response::builder().code(404).body("probe").build())
        		.on_start(|board_server| {
        			let other = Other { value = Shared::new(0) };
        			Server::builder()
        				.port(0)
        				.with_service(Service::new(other.dispatcher().into_protocol(json_codec())))
        				.on_request(|request| Response::builder().code(404).body("probe").build())
        				.on_start(|other_server| drive(board_server.port(), other_server.port()))
        				.build()
        				.start();
        		})
        		.build()
        		.start();
        }
        
        fun drive(board_port: i32, other_port: i32) {
        	// One call: socket + contract enforcement + attach + mirrors.
        	match Client::connect(i"ws://localhost:{board_port}", json_codec()) {
        		Ok(let client) => {
        			// Typed mirrors: values arrive decoded at each field's type.
        			let counting = client.count.sub(|n| {
        				print(i"count = {n}");
        			});
        			let labeling = client.label.sub(|s| {
        				if s != "" {
        					print(i"label = {s}");
        				}
        			});
        			match client.add(7) {
        				Ok(let n) => print(i"add -> {n}"),
        				Err(let error) => print(i"add err {error.to_json()}"),
        			}
        			sleep(300);
        			// Drift: a Board client pointed at Other's server refuses cleanly.
        			match Client::connect(i"ws://localhost:{other_port}", json_codec()) {
        				Ok(let wrong) => print("drift NOT caught"),
        				Err(let error) => print(i"drift: {error.to_json()}"),
        			}
        			sleep(100);
        			exit(0);
        		},
        		Err(let error) => {
        			print(i"connect failed: {error.to_json()}");
        			exit(1);
        		},
        	}
        }

        "#,
        "count = 0\ncount = 7\nlabel = sum 7\nadd -> 7\ndrift: {\"Contract\":\"the server reports a different service surface\"}\n",
    );
}

// --- B168: a trait bound over a BARE parameter, resolved in a generic body ---
//
// A33 widened `std::ui`'s read-only bindings from `Signal<T>` to a `Source<T>`
// bound, and `View::swap` — read-only like every other, no write anywhere in
// it — was the one site that could NOT come along. The gap the widening walked
// into was narrow and exact:
//
//   * `S: Source<List<T>>` — the bound's argument CONSTRUCTED over the caller's
//     own `T` — resolved fine inside a generic body. That is `bind_each`, and
//     it shipped.
//   * `S: Source<T>` — the bound's argument the BARE parameter — did not. The
//     callee's `T` was inferred through the bound to the *impl's* own unbound
//     parameter instead of to the caller's, so the callee's `T: PartialEq` was
//     then checked against something that carries no bound and refused.
//
// `swap_split` calls `self.swap(gated, render)` from exactly such a body
// (`gated: Signal<T>`, `T` its own parameter), so widening `swap` made std
// itself uncompilable — with an explicit `self.swap<T, Signal<T>>(..)` too, the
// bound check being downstream of the argument. The value FLOWED correctly:
// dropping `T`'s bound entirely compiled and ran the same program, which placed
// the defect in the bound CHECK rather than in inference.
//
// B168's cause: recovering a bound's arguments from the receiver's impl
// (`analyzer.rs::trait_args_for`) reconciles the receiver against the impl's
// SUBJECT, and `reconcile_type` is a unifier — undirected. One generic side and
// one concrete side can only bind one way, which is why the constructed
// argument always worked; two generic sides kept the LEFT one's binding
// (`caller T -> impl Z`), which says nothing about the impl's `Z` and left its
// `with Source<Z>` ungrounded. The bindings are now ORIENTED toward the impl's
// own binders, so the bound's argument comes back as the caller's parameter —
// constraints and all.
//
// With it fixed, `View::swap`, `View::swap_split` and `ui::chunk_preload`
// widened together, keeping their generic-parameter ORDER — the split gate
// rebinds their type arguments by position
// (`transformer.rs::rebind_by_position`).

/// The gap, minimized out of `swap_split`. B168 closed it: the impl's binders
/// are what a receiver/subject reconciliation has to bind, and a bare-parameter
/// receiver argument used to bind the caller's parameter to them instead.
#[test]
fn a_bare_parameter_source_bound_resolves_inside_a_generic_body() {
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::reactive::{ Signal, Source };

        fun consume<T: PartialEq, S: Source<T>>(source: S): T {
            source.get()
        }

        fun wrapper<T: PartialEq>(value: T): T {
            let cell: Signal<T> = Signal::new(value);
            consume(cell)
        }

        fun main() { print(wrapper(1)); }
        main();
        "#,
        "1\n",
    );
}

/// The half that ALWAYS worked, kept beside it so the pair localizes the gap to
/// the bare parameter rather than to `Source` bounds in general — this is
/// `bind_each`'s shape, and the control the fix must not move.
#[test]
fn a_constructed_source_bound_resolves_inside_a_generic_body() {
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::reactive::{ Signal, Source };

        fun consume<T: PartialEq, S: Source<List<T>>>(source: S): i32 {
            source.get().len()
        }

        fun wrapper<T: PartialEq>(value: List<T>): i32 {
            let cell: Signal<List<T>> = Signal::new(value);
            consume(cell)
        }

        fun main() { print(wrapper([1, 2, 3])); }
        main();
        "#,
        "3\n",
    );
}

/// The NON-VACUITY pin, and the one that matters most: orienting the bindings
/// must not turn the bound check off. The caller's own `T` carries NO bound
/// here, so the callee's `T: PartialEq` is genuinely unsatisfied and the call
/// is still refused — naming the caller's parameter, which is now the parameter
/// the check actually reaches.
#[test]
fn a_bare_parameter_source_bound_still_refuses_an_unbounded_caller() {
    assert_fails_with(
        r#"
        import std::compare::PartialEq;
        import std::reactive::{ Signal, Source };

        fun consume<T: PartialEq, S: Source<T>>(source: S): T {
            source.get()
        }

        fun wrapper<T>(value: T): T {
            let cell: Signal<T> = Signal::new(value);
            consume(cell)
        }

        fun main() { print(wrapper(1)); }
        main();
        "#,
        "generic parameter 'T' is missing the bound ': PartialEq'",
    );
}

/// The bound's argument reaches a MEMBER through the caller's parameter, not
/// just the bound check: `T: Show` arriving intact means `value.show()` inside
/// the callee resolves to the caller's impl. A `T` bound to the impl's own
/// unbound parameter had no `show` at all, so this is the same defect read from
/// the other end — and it RUNS, so the value is the caller's too.
#[test]
fn a_bare_parameter_bound_carries_the_callers_member_into_the_callee() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ Signal, Source };

        trait Show { fun show(self): str; }
        impl i32 with Show { fun show(self): str { i"[{self}]" } }

        fun consume<T: Show, S: Source<T>>(source: S): str {
            source.get().show()
        }

        fun wrapper<T: Show>(value: T): str {
            let cell: Signal<T> = Signal::new(value);
            consume(cell)
        }

        fun main() { print(wrapper(7)); }
        main();
        "#,
        "[7]\n",
    );
}

/// The MULTI-PARAMETER edge: two bare-parameter bounds in one signature, each
/// carrying a different bound, over an impl that writes its binders in the
/// OTHER order (`impl Pair<type A, type B> with Feed<B>`). Orienting by
/// position rather than by identity would swap them here; orienting to the
/// binder each pair actually names does not.
#[test]
fn two_bare_parameter_bounds_bind_their_own_arguments() {
    assert_compiles_and_runs(
        r#"
        trait First { fun first(self): str; }
        trait Second { fun second(self): str; }
        impl i32 with First { fun first(self): str { i"first {self}" } }
        impl str with Second { fun second(self): str { i"second {self}" } }

        trait Feed<T> { fun feed(self): T; }

        struct Pair<A, B> { left: A, right: B }

        impl Pair<type A, type B> with Feed<B> {
            fun feed(self): B { self.right }
        }

        impl Pair<type A, type B> {
            fun new(left: A, right: B): Pair<A, B> { Pair { left, right } }
        }

        fun consume<X: First, Y: Second, F: Feed<Y>>(feeder: F, marker: X): str {
            i"{marker.first()} / {feeder.feed().second()}"
        }

        fun wrapper<X: First, Y: Second>(marker: X, value: Y): str {
            let pair: Pair<X, Y> = Pair::new(marker, value);
            consume(pair, marker)
        }

        fun main() { print(wrapper(1, "two")); }
        main();
        "#,
        "first 1 / second two\n",
    );
}

/// The NESTED edge: the bound's argument travels two generic bodies deep, each
/// re-wrapping the value. Every hop is a fresh receiver/subject reconciliation
/// with generics on both sides, so a fix that repaired only the first hop shows
/// up here.
#[test]
fn a_bare_parameter_bound_survives_two_generic_bodies() {
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::reactive::{ Signal, Source };

        fun inner<T: PartialEq, S: Source<T>>(source: S): T {
            source.get()
        }

        fun middle<T: PartialEq, S: Source<T>>(source: S): T {
            let cell: Signal<T> = Signal::new(source.get());
            inner(cell)
        }

        fun outer<T: PartialEq>(value: T): T {
            let cell: Signal<T> = Signal::new(value);
            middle(cell)
        }

        fun main() { print(outer(5)); }
        main();
        "#,
        "5\n",
    );
}

/// A BLANKET impl reached from a generic body: the subject IS the binder, so
/// the reconciliation has a generic on both sides at the TOP level rather than
/// inside a nominal type's arguments. B168 fixed this half — before it, the
/// error was `'T' is missing the bound ': Tag'`, the callee's `T` resolved to
/// the impl's own binder — and left a DIFFERENT one open, which is what this
/// pin now names: `W: Wrap<T>` is checked against the caller's `T`, and
/// `satisfies_trait_bound` answers for a `Type::Generic` value from its
/// DECLARED bounds alone, never from an impl. A blanket impl covers every type
/// including an abstract parameter, so the bound holds and the check cannot see
/// it. Un-ignore when a generic value is allowed to satisfy a blanket impl.
#[test]
#[ignore = "B173: `satisfies_trait_bound` answers a `Type::Generic` value \
            from its declared bounds alone, so a blanket `impl type T with \
            Wrap<T>` cannot satisfy `W: Wrap<T>` when `W` is bound to the \
            caller's own parameter. A concrete caller passes; whether an \
            abstract parameter may satisfy a blanket impl is B173's ruling."]
fn a_blanket_impl_bound_resolves_from_a_generic_body() {
    assert_compiles_and_runs(
        r#"
        trait Tag { fun tag(self): str; }
        impl i32 with Tag { fun tag(self): str { i"<{self}>" } }

        trait Wrap<T> { fun unwrap(self): T; }
        impl type T with Wrap<T> { fun unwrap(self): T { self } }

        fun consume<T: Tag, W: Wrap<T>>(wrapped: W): str {
            wrapped.unwrap().tag()
        }

        fun wrapper<T: Tag>(value: T): str {
            consume(value)
        }

        fun main() { print(wrapper(3)); }
        main();
        "#,
        "<3>\n",

// --- M16: a T-independent generic body is emitted ONCE ----------------------

/// Counts the top-level `function` declarations in `js` whose body — the lines
/// up to the closing brace at column 0 — contains `needle`.
fn emitted_bodies_containing(js: &str, needle: &str) -> usize {
    let mut count = 0;
    let mut lines = js.lines().peekable();
    while let Some(line) = lines.next() {
        if !(line.starts_with("function ") || line.starts_with("async function ")) {
            continue;
        }
        let mut body = String::new();
        for inner in lines.by_ref() {
            if inner == "}" {
                break;
            }
            body.push_str(inner);
            body.push('\n');
        }
        if body.contains(needle) {
            count += 1;
        }
    }
    count
}

/// M16 (audit run 6's F18). A generic function whose EMITTED body does not
/// depend on `T` is one function, however many types it is instantiated at —
/// and it used to be one byte-identical JS copy per instantiation.
///
/// The subject is the shape the item was filed on: `scoped_file<T>` behind the
/// five `with_file*` forms, whose nine emitted lines mention no type at all
/// and which `file.mjs` carried two copies of. Nothing here reasons about
/// which of the transformer's per-monomorphization decisions the body
/// consulted — the bodies are compared as EMITTED, which is exact where a
/// T-independence analysis would have to be conservative.
#[test]
fn a_t_independent_generic_body_is_emitted_once_for_all_its_instantiations() {
    let js = compile(
        r#"
        import std::io::print;

        fun apply_twice<T>(value: T, step: |T| T): T {
            let once = step(value);
            step(once)
        }

        fun main() {
            print(apply_twice(1, |n| n + 1));
            print(apply_twice("a", |s| s + "!"));
            print(apply_twice(true, |b| b));
        }
        main();
        "#,
    )
    .expect("compiles");
    assert_eq!(
        emitted_bodies_containing(&js, "const once = step(value);"),
        1,
        "three instantiations of a T-independent body must share ONE emission:\n{js}"
    );
}

/// The control, and the thing that keeps the pin above from being a claim that
/// generics stopped monomorphizing: a body that DOES resolve differently per
/// type still gets one emission per type. Here each `T`'s `to_string` is a
/// different function, so the two bodies do not render alike and neither may
/// stand in for the other.
#[test]
fn a_t_dependent_generic_body_is_still_emitted_once_per_instantiation() {
    let js = compile(
        r#"
        import std::io::print;
        import std::display::{ Display, format };

        struct Metres { value: i32 }
        struct Seconds { value: i32 }

        impl Metres with Display { fun to_string(self): str { format(self.value) + "m" } }
        impl Seconds with Display { fun to_string(self): str { format(self.value) + "s" } }

        fun label<T: Display>(value: T): str {
            let rendered = value.to_string();
            "[" + rendered + "]"
        }

        fun main() {
            print(label(Metres { value = 3 }));
            print(label(Seconds { value = 4 }));
        }
        main();
        "#,
    )
    .expect("compiles");
    assert_eq!(
        emitted_bodies_containing(&js, "const rendered ="),
        2,
        "two instantiations that resolve to DIFFERENT code must stay two \
         emissions:\n{js}"
    );
}

/// And the shared body has to RUN at every type it was shared across — the
/// assertion the emission count cannot make.
#[test]
fn a_shared_generic_body_runs_at_every_type_it_was_shared_across() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun apply_twice<T>(value: T, step: |T| T): T {
            let once = step(value);
            step(once)
        }

        fun main() {
            print(apply_twice(1, |n| n + 1));
            print(apply_twice("a", |s| s + "!"));
        }
        main();
        "#,
        "3\na!!\n",
    );
}
