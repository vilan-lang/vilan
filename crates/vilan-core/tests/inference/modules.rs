//! Rule-4 completion, trait-signature conformance (B29), server-side rendering,
//! path semantics, and module initialization order (B33).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- rule-4 completion S2: the `bumps` effect (rule4-completion.md §1, C6) ---
//
// Inference-only this slice — no enforcement consumer exists until S3 keys E2
// off the verdict — so these pins read the inferred sets straight off the
// analysis result.

/// The inferred `bumps` positions per function name (user functions and
/// externs merged): S2's observable. Analysis-only — no transform — so a test
/// source may declare bodyless externs freely. Panics on analysis errors.
fn bumps_of(source: &str) -> std::collections::HashMap<String, Vec<u32>> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(
                errors.is_empty(),
                "expected a clean analysis, got: {:#?}",
                errors
                    .into_iter()
                    .map(|error| error.msg)
                    .collect::<Vec<_>>()
            );
            let program = program.expect("analysis produced no program");
            let mut bumps: std::collections::HashMap<String, Vec<u32>> = program
                .functions
                .values()
                .map(|function| {
                    (
                        function.name.to_string(),
                        function.bumps.iter().copied().collect(),
                    )
                })
                .collect();
            for external in program.external_functions.values() {
                bumps.insert(
                    external.name.to_string(),
                    external.bumps.iter().copied().collect(),
                );
            }
            bumps
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

#[track_caller]
fn assert_bumps(source: &str, function_name: &str, expected: &[u32]) {
    let bumps = bumps_of(source);
    let Some(actual) = bumps.get(function_name) else {
        panic!("function '{function_name}' not in the analysis result");
    };
    assert_eq!(
        actual, expected,
        "bumps positions for '{function_name}' (expected {expected:?}, got {actual:?})"
    );
}

#[test]
fn bumps_list_push_bumps_the_receiver() {
    // The table's `List::push` row flows through the caller: `touch` bumps xs.
    assert_bumps(
        "fun touch(xs: &mut List<i32>) { xs.push(1); }\nfun main() { mut xs = [ 1 ]; touch(&mut xs); }\n",
        "touch",
        &[0],
    );
}

#[test]
fn bumps_list_pop_bumps_the_receiver() {
    assert_bumps(
        "fun shrink(xs: &mut List<i32>) { xs.pop(); }\nfun main() { mut xs = [ 1 ]; shrink(&mut xs); }\n",
        "shrink",
        &[0],
    );
}

#[test]
fn bumps_map_insert_and_remove_bump() {
    let source = r#"
        import std::map::Map;
        fun put(m: &mut Map<str, i32>) { m.insert("k", 1); }
        fun evict(m: &mut Map<str, i32>) { m.remove("k"); }
        fun main() {
            mut m: Map<str, i32> = Map::new();
            put(&mut m);
            evict(&mut m);
        }
    "#;
    assert_bumps(source, "put", &[0]);
    assert_bumps(source, "evict", &[0]);
}

#[test]
fn bumps_set_insert_and_remove_bump() {
    let source = r#"
        import std::set::Set;
        fun add(s: &mut Set<i32>) { s.insert(1); }
        fun take_out(s: &mut Set<i32>) { s.remove(1); }
        fun main() {
            mut s: Set<i32> = Set::new();
            add(&mut s);
            take_out(&mut s);
        }
    "#;
    assert_bumps(source, "add", &[0]);
    assert_bumps(source, "take_out", &[0]);
}

#[test]
fn bumps_arena_insert_and_remove_bump_but_set_is_stable() {
    // The one stable native row: `Arena::set` overwrites a live slot in place —
    // geometry intact — while insert grows/reuses slots and remove frees one.
    let source = r#"
        import std::arena::{ Arena, Handle };
        fun grow(a: &mut Arena<i32>): Handle<i32> { a.insert(1) }
        fun overwrite(a: &mut Arena<i32>, h: Handle<i32>) { a.set(h, 5); }
        fun free(a: &mut Arena<i32>, h: Handle<i32>) { a.remove(h); }
        fun main() {
            mut a: Arena<i32> = Arena::new();
            let h = grow(&mut a);
            overwrite(&mut a, h);
            free(&mut a, h);
        }
    "#;
    assert_bumps(source, "grow", &[0]);
    assert_bumps(source, "overwrite", &[]);
    assert_bumps(source, "free", &[0]);
}

#[test]
fn bumps_field_writes_are_content_stable() {
    assert_bumps(
        "struct Point { x: i32, y: i32 }\nfun retag(p: &mut Point) { p.x = 1; }\nfun main() { mut p = Point { x = 0, y = 0 }; retag(&mut p); }\n",
        "retag",
        &[],
    );
}

#[test]
fn bumps_element_writes_are_content_stable() {
    // A subscript write replaces contents in the surviving slot — §2's element
    // rule; the path has an Index, so the aggregate-reassignment rule stays out.
    assert_bumps(
        "fun blank(xs: &mut List<i32>) { xs[0] = 9; }\nfun main() { mut xs = [ 1 ]; blank(&mut xs); }\n",
        "blank",
        &[],
    );
}

#[test]
fn bumps_whole_reassignment_through_the_view_bumps() {
    // Whole replacement through a view parameter is the BARE assignment
    // (transparent references write through; `*xs = …` is rejected with a steer)
    // — and it swaps the entire aggregate: bumping.
    assert_bumps(
        "fun reset(xs: &mut List<i32>) { xs = [ 0 ]; }\nfun main() { mut xs = [ 1 ]; reset(&mut xs); }\n",
        "reset",
        &[0],
    );
}

#[test]
fn bumps_aggregate_field_reassignment_bumps() {
    // Swapping an aggregate field detaches every interior view (§6.0's
    // aggregate-owner event) — bumping, unlike the scalar field write above.
    assert_bumps(
        "struct Holder { inner: List<i32> }\nfun swap_inner(h: &mut Holder) { h.inner = [ 0 ]; }\nfun main() { mut h = Holder { inner = [ 1 ] }; swap_inner(&mut h); }\n",
        "swap_inner",
        &[0],
    );
}

#[test]
fn bumps_propagates_through_a_forwarding_call() {
    // The fixpoint chains: `forward` passes its parameter to bumping `touch`.
    let source = "fun touch(xs: &mut List<i32>) { xs.push(1); }\nfun forward(xs: &mut List<i32>) { touch(xs); }\nfun main() { mut xs = [ 1 ]; forward(&mut xs); }\n";
    assert_bumps(source, "forward", &[0]);
}

#[test]
fn bumps_extern_off_table_defaults_to_bumping() {
    // A bodyless extern with a `&mut` parameter may do anything — the safe
    // default — and the verdict propagates to its caller.
    let source = "external fun grow(xs: &mut List<i32>);\nfun call_it(xs: &mut List<i32>) { grow(xs); }\nfun main() { mut xs = [ 1 ]; call_it(&mut xs); }\n";
    assert_bumps(source, "grow", &[0]);
    assert_bumps(source, "call_it", &[0]);
}

#[test]
fn bumps_dispatched_callee_defaults_to_bumping() {
    // A trait method on a generic receiver is unresolvable at inference time —
    // the receiver defaults to bumping even though this impl only field-writes.
    let source = r#"
        trait Poke {
            fun wiggle(&mut self);
        }
        struct Cell { value: i32 }
        impl Cell with Poke {
            fun wiggle(&mut self) { self.value = 1; }
        }
        fun tickle<T: Poke>(x: &mut T) { x.wiggle(); }
        fun main() {
            mut c = Cell { value = 0 };
            tickle(&mut c);
        }
    "#;
    assert_bumps(source, "tickle", &[0]);
}

// --- rule-4 completion S3: anchoring + the E2 swap (C10 + C6) ----------------
// Call-returned views and wrapped-view captures now anchor at their projected
// roots (compute_view_origins reads the S1 root-sets at call sites), and E2
// fires on the callee's S2 `bumps` verdict instead of the bare `&mut`
// convention. These pins are the liveness proof in both directions: the C10
// shapes reject, the C6 relaxations accept.

#[test]
fn a_bumping_call_under_a_live_borrows_call_view_is_rejected() {
    // The canonical C10 shape: `let v = at(&mut xs, 0)` anchors v at xs, so a
    // later push fires E2 exactly as a direct `&mut xs[0]` view always did.
    assert_fails_with(
        r#"
        fun at(xs: &mut List<i32>, index: i32): &mut i32 {
            &mut xs[index]
        }
        fun main() {
            mut xs = [ 1, 2 ];
            let v = at(&mut xs, 0);
            xs.push(3);
            v = 9;
        }
        main();
        "#,
        "while a view into it is live",
    );
}

#[test]
fn reassigning_the_root_under_a_live_borrows_call_view_is_rejected() {
    // E1 through the anchored view: whole-root reassignment, not a call.
    assert_fails_with(
        r#"
        fun at(xs: &mut List<i32>, index: i32): &mut i32 {
            &mut xs[index]
        }
        fun main() {
            mut xs = [ 1, 2 ];
            let v = at(&mut xs, 0);
            xs = [ 0 ];
            v = 9;
        }
        main();
        "#,
        "cannot reassign",
    );
}

#[test]
fn holding_a_borrows_call_view_across_await_is_rejected() {
    // E3 sees the anchored binding: re-acquire after the suspension.
    assert_fails_with(
        r#"
        import std::time::sleep;
        fun at(xs: &mut List<i32>, index: i32): &mut i32 {
            &mut xs[index]
        }
        async fun work() {
            mut xs = [ 1 ];
            let v = at(&mut xs, 0);
            await sleep(1);
            v = 9;
        }
        fun main() { work(); }
        main();
        "#,
        "across 'await'",
    );
}

#[test]
fn a_mutation_of_a_sibling_root_under_a_borrows_call_view_is_accepted() {
    // The anchor is precise: pushing a DIFFERENT list never touches v's root.
    assert_compiles(
        r#"
        fun at(xs: &mut List<i32>, index: i32): &mut i32 {
            &mut xs[index]
        }
        fun main() {
            mut xs = [ 1 ];
            mut ys = [ 2 ];
            let v = at(&mut xs, 0);
            ys.push(3);
            v = 9;
        }
        main();
        "#,
    );
}

#[test]
fn a_multi_root_projection_anchors_at_every_branch_root() {
    // A view projecting either parameter by branch anchors at BOTH roots — a
    // bumping call on the second root fires even when the first was taken.
    assert_fails_with(
        r#"
        fun pick(a: &mut List<i32>, b: &mut List<i32>, first: bool): &mut i32 {
            if first { &mut a[0] } else { &mut b[0] }
        }
        fun main() {
            mut xs = [ 1 ];
            mut ys = [ 2 ];
            let v = pick(&mut xs, &mut ys, true);
            ys.push(3);
            v = 9;
        }
        main();
        "#,
        "while a view into it is live",
    );
}

#[test]
fn a_content_stable_call_under_a_live_view_is_accepted() {
    // The C6 relaxation clearing E2's recorded scalar-field conservatism: a
    // `&mut s` callee that only field-writes cannot invalidate the held field
    // view, so the call is now legal (it rejected under the convention proxy).
    assert_compiles(
        r#"
        struct Point { x: i32, y: i32 }
        fun retag(p: &mut Point) {
            p.x = 1;
        }
        fun main() {
            mut p = Point { x = 0, y = 0 };
            let v = &mut p.y;
            retag(&mut p);
            v = 9;
        }
        main();
        "#,
    );
}

#[test]
fn a_bumping_user_call_under_a_live_view_is_still_rejected() {
    // The reject twin of the relaxation: same shape, but the callee reassigns
    // an aggregate field — a bump — so E2 still fires.
    assert_fails_with(
        r#"
        struct Holder { inner: List<i32>, tag: i32 }
        fun swap_inner(h: &mut Holder) {
            h.inner = [ 0 ];
        }
        fun main() {
            mut h = Holder { inner = [ 1 ], tag = 0 };
            let v = &mut h.tag;
            swap_inner(&mut h);
            v = 9;
        }
        main();
        "#,
        "while a view into it is live",
    );
}

// --- rule-4 completion S4: the iterator chain -------------------------------
// `for e in &mut user_container` bindings anchor at the container via the
// ForEach origin arm (which predates S3 and covers user containers driving
// `next_mut`); these pins prove the chain holds end to end.

#[test]
fn a_bumping_call_on_a_user_container_inside_for_mut_is_rejected() {
    // The loop binding e anchors at `bag`, and `push` (through the wrapper's
    // inferred bumps) fires E2 mid-iteration.
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };
        struct Bag { items: List<i32>, cursor: i32 }
        impl Bag {
            fun next_mut(&mut self): Option<&mut i32> {
                if self.cursor < self.items.len() {
                    let index = self.cursor;
                    self.cursor = self.cursor + 1;
                    Some(&mut self.items[index])
                } else {
                    None
                }
            }
            fun add(&mut self, value: i32) {
                self.items.push(value);
            }
        }
        fun main() {
            mut bag = Bag { items = [ 1, 2 ], cursor = 0 };
            for e in &mut bag {
                bag.add(3);
                e = 9;
            }
        }
        main();
        "#,
        "while a view into it is live",
    );
}

#[test]
fn a_content_stable_call_on_a_user_container_inside_for_mut_is_accepted() {
    // The C6 twin one hop up: a cursor-reset is a scalar field write —
    // content-stable — so calling it mid-iteration is legal.
    assert_compiles(
        r#"
        import std::option::Option::{ self, Some, None };
        struct Bag { items: List<i32>, cursor: i32 }
        impl Bag {
            fun next_mut(&mut self): Option<&mut i32> {
                if self.cursor < self.items.len() {
                    let index = self.cursor;
                    self.cursor = self.cursor + 1;
                    Some(&mut self.items[index])
                } else {
                    None
                }
            }
            fun mark(&mut self) {
                self.cursor = self.cursor;
            }
        }
        fun main() {
            mut bag = Bag { items = [ 1, 2 ], cursor = 0 };
            for e in &mut bag {
                bag.mark();
                e = 9;
            }
        }
        main();
        "#,
    );
}

#[test]
fn a_bump_inside_a_tuple_comprehension_is_rejected() {
    // The review-block pin: `scan_bumps` initially omitted the
    // TupleComprehension arm, so an aggregate-field swap inside a comprehension
    // body read as content-stable and E2 silently permitted it — with an
    // observable stale write-through on JS. The comprehension's source and body
    // are executable like any other sub-expression.
    assert_fails_with(
        r#"
        struct Holder { inner: List<i32> }
        fun sneaky<T: (2..)>(h: &mut Holder, sources: (U in T: List<U>)): T {
            (source in sources => { h.inner = [ 0 ]; source.len() })
        }
        fun main() {
            mut h = Holder { inner = [ 100, 200 ] };
            let v = &mut h.inner[0];
            let _ = sneaky(&mut h, ([ 1, 2, 3 ], [ "a", "b" ]));
            v = 9;
        }
        main();
        "#,
        "while a view into it is live",
    );
}

// --- B29: full trait-signature conformance -----------------------------------
// The checker used to accept any impl whose members matched a trait by NAME
// only; these pin the general per-member signature check (receiver convention,
// arity, parameter conventions/types under {Self -> subject, trait generics ->
// with-clause args}, and return type). Asyncness is deliberately NOT enforced
// (`a_declared_async_impl_of_a_sync_trait_method_is_permitted`).

#[test]
fn a_by_value_receiver_against_a_ref_declaration_is_rejected() {
    assert_fails_with(
        r#"
        trait Speak { fun say(&self): str; }
        struct Cat {}
        impl Cat with Speak { fun say(self): str { "meow" } }
        fun main() { let c = Cat {}; }
        "#,
        "match the receiver convention",
    );
}

#[test]
fn a_ref_receiver_against_a_ref_mut_declaration_is_rejected() {
    assert_fails_with(
        r#"
        trait Bump { fun bump(&mut self): void; }
        struct Counter { n: i32 }
        impl Counter with Bump { fun bump(&self): void {} }
        fun main() { let c = Counter { n = 0 }; }
        "#,
        "match the receiver convention",
    );
}

#[test]
fn a_ref_mut_receiver_against_an_own_declaration_is_rejected() {
    assert_fails_with(
        r#"
        trait Consume { fun consume(own self): void; }
        struct Box2 {}
        impl Box2 with Consume { fun consume(&mut self): void {} }
        fun main() { let b = Box2 {}; }
        "#,
        "match the receiver convention",
    );
}

#[test]
fn a_matching_receiver_convention_compiles() {
    assert_compiles(
        r#"
        trait Speak { fun say(&self): str; }
        struct Cat {}
        impl Cat with Speak { fun say(&self): str { "meow" } }
        fun main() { let c = Cat {}; }
        "#,
    );
}

#[test]
fn an_impl_with_too_few_parameters_is_rejected() {
    assert_fails_with(
        r#"
        trait Handler2 { fun handle(&self, x: i32): void; }
        struct H {}
        impl H with Handler2 { fun handle(&self): void {} }
        fun main() { let h = H {}; }
        "#,
        "match the declared parameter list",
    );
}

#[test]
fn an_impl_with_too_many_parameters_is_rejected() {
    assert_fails_with(
        r#"
        trait Handler2 { fun handle(&self, x: i32): void; }
        struct H {}
        impl H with Handler2 { fun handle(&self, x: i32, y: i32): void {} }
        fun main() { let h = H {}; }
        "#,
        "match the declared parameter list",
    );
}

#[test]
fn a_parameter_convention_mismatch_is_rejected() {
    assert_fails_with(
        r#"
        trait Handler2 { fun handle(&self, x: i32): void; }
        struct H {}
        impl H with Handler2 { fun handle(&self, x: &i32): void {} }
        fun main() { let h = H {}; }
        "#,
        "match the parameter convention",
    );
}

#[test]
fn a_parameter_type_mismatch_is_rejected() {
    assert_fails_with(
        r#"
        trait Handler2 { fun handle(&self, x: i32): void; }
        struct H {}
        impl H with Handler2 { fun handle(&self, x: str): void {} }
        fun main() { let h = H {}; }
        "#,
        "match the declared type",
    );
}

#[test]
fn a_return_type_mismatch_is_rejected() {
    assert_fails_with(
        r#"
        trait Producer { fun make(&self): i32; }
        struct P {}
        impl P with Producer { fun make(&self): str { "x" } }
        fun main() { let p = P {}; }
        "#,
        "match the declared return type",
    );
}

#[test]
fn a_self_typed_parameter_at_a_concrete_type_compiles() {
    // `Self` in the trait declaration substitutes to the impl's subject, so an
    // impl spelling the concrete type conforms.
    assert_compiles(
        r#"
        trait Eq2 { fun eq(self, other: Self): bool; }
        struct Point { x: i32 }
        impl Point with Eq2 { fun eq(self, other: Point): bool { self.x == other.x } }
        fun main() { let p = Point { x = 1 }; }
        "#,
    );
}

#[test]
fn a_self_typed_parameter_at_the_wrong_type_is_rejected() {
    assert_fails_with(
        r#"
        trait Eq2 { fun eq(self, other: Self): bool; }
        struct Point { x: i32 }
        struct Other {}
        impl Point with Eq2 { fun eq(self, other: Other): bool { true } }
        fun main() { let p = Point { x = 1 }; }
        "#,
        "match the declared type",
    );
}

#[test]
fn a_parameterized_traits_generic_through_the_with_clause_compiles() {
    // `From2<T>`'s `T` substitutes to the `with`-clause argument (`Feet`), so an
    // impl whose parameter is `Feet` conforms.
    assert_compiles(
        r#"
        trait From2<T> { fun from(value: T): Self; }
        struct Meters {}
        struct Feet {}
        impl Meters with From2<Feet> { fun from(value: Feet): Meters { Meters {} } }
        fun main() { let m = Meters {}; }
        "#,
    );
}

#[test]
fn a_parameterized_traits_generic_at_the_wrong_type_is_rejected() {
    assert_fails_with(
        r#"
        trait From2<T> { fun from(value: T): Self; }
        struct Meters {}
        struct Feet {}
        struct Yards {}
        impl Meters with From2<Feet> { fun from(value: Yards): Meters { Meters {} } }
        fun main() { let m = Meters {}; }
        "#,
        "match the declared type",
    );
}

#[test]
fn a_generic_method_with_a_wrong_type_parameter_count_is_rejected() {
    // The structural half of a generic member's alpha-equivalence: the type-
    // parameter lists must match in arity.
    assert_fails_with(
        r#"
        trait Mapper { fun go<T>(&self, x: T): T; }
        struct S {}
        impl S with Mapper { fun go(&self, x: i32): i32 { x } }
        fun main() { let s = S {}; }
        "#,
        "match the trait's type-parameter list",
    );
}

#[test]
fn a_generic_method_with_matching_structure_compiles() {
    assert_compiles(
        r#"
        trait Mapper { fun go<T>(&self, x: T): T; }
        struct S {}
        impl S with Mapper { fun go<U>(&self, x: U): U { x } }
        fun main() { let s = S {}; }
        "#,
    );
}

/// B29 residue, closed: a member's own generic parameters are RIGID under
/// conformance (`compare_type_rigid`) — a trait promising to accept any `T` is
/// not implemented by one fixing that position to `str`. Before the fix an
/// unbounded generic compared equal to any concrete type and this passed.
#[test]
fn a_generic_method_fixing_a_generic_parameter_to_a_concrete_type_is_rejected() {
    assert_fails_with(
        r#"
        trait Mapper { fun go<T>(&self, x: T): i32; }
        struct S {}
        impl S with Mapper { fun go<T>(&self, x: str): i32 { 0 } }
        fun main() { let s = S {}; }
        "#,
        "match the declared type",
    );
}

#[test]
fn omitting_a_default_bodied_member_compiles() {
    // A trait member WITH a default body is inherited; an impl need not restate
    // it, and providing only the required member conforms.
    assert_compiles(
        r#"
        trait Greeter2 {
            fun name(&self): str;
            fun greet(&self): str { "hi" }
        }
        struct G {}
        impl G with Greeter2 { fun name(&self): str { "g" } }
        fun main() { let g = G {}; }
        "#,
    );
}

#[test]
fn overriding_a_default_bodied_member_conformingly_compiles() {
    assert_compiles(
        r#"
        trait Greeter2 {
            fun name(&self): str;
            fun greet(&self): str { "hi" }
        }
        struct G {}
        impl G with Greeter2 {
            fun name(&self): str { "g" }
            fun greet(&self): str { "hello" }
        }
        fun main() { let g = G {}; }
        "#,
    );
}

#[test]
fn overriding_a_default_bodied_member_with_a_bad_signature_is_rejected() {
    // An override conforms like any required member — a mismatched receiver on
    // the override is caught.
    assert_fails_with(
        r#"
        trait Greeter2 {
            fun name(&self): str;
            fun greet(&self): str { "hi" }
        }
        struct G {}
        impl G with Greeter2 {
            fun name(&self): str { "g" }
            fun greet(self): str { "hello" }
        }
        fun main() { let g = G {}; }
        "#,
        "match the receiver convention",
    );
}

#[test]
fn a_declared_async_impl_of_a_sync_trait_method_is_permitted() {
    // Asyncness agreement is NOT enforced (the WO's escape hatch): dispatch is
    // monomorphized and `async_infer` propagates asyncness through the contract,
    // so a caller awaits regardless of the trait's declared asyncness — std's
    // `SplitDuplex::send` (async body) impls the sync-declared `DuplexTransport::
    // send` exactly this way and is sound. An async impl of a sync declaration
    // therefore compiles.
    assert_compiles(
        r#"
        trait T { fun m(&self): void; }
        struct S {}
        impl S with T { async fun m(&self): void {} }
        fun main() { let s = S {}; }
        "#,
    );
}

#[test]
fn a_std_drop_with_a_by_value_receiver_is_caught_by_the_general_rule() {
    // S2a's original shape (`fun drop(self)` against `fun drop(&mut self)`) — the
    // GENERAL conformance rule rejects it independently of the targeted
    // `check_drop_signature` (both fire; this pins the general rule's message).
    assert_fails_with(
        r#"
        import std::drop::Drop;
        resource struct R { handle: i32 }
        impl R with Drop { fun drop(self) {} }
        fun main() { let r = R { handle = 1 }; }
        "#,
        "match the receiver convention",
    );
}

#[test]
fn a_user_defined_trait_named_drop_conforms_on_its_own_terms() {
    // A user's own `trait Drop` (a different entity than std's) declares
    // `fun drop(self)`, so an impl providing `fun drop(self)` conforms — the
    // general rule checks against the user's declaration, not std's.
    assert_compiles(
        r#"
        trait Drop { fun drop(self); }
        struct X {}
        impl X with Drop { fun drop(self) {} }
        fun main() { let x = X {}; }
        "#,
    );
}

// --- B29 review additions --------------------------------------------------

#[test]
fn a_static_trait_member_conforms_positionally() {
    // A receiver-less (static) trait member compares position-for-position like
    // any other — the FromJson::from_json shape.
    assert_compiles(
        r#"
        trait Maker {
            fun make(seed: i32): Self;
        }
        struct Box { value: i32 }
        impl Box with Maker {
            fun make(seed: i32): Box { Box { value = seed } }
        }
        fun main() { let b = Box::make(1); }
        "#,
    );
}

#[test]
fn a_static_trait_member_type_mismatch_is_rejected() {
    assert_fails_with(
        r#"
        trait Maker {
            fun make(seed: i32): Self;
        }
        struct Box { value: i32 }
        impl Box with Maker {
            fun make(seed: str): Box { Box { value = 1 } }
        }
        fun main() {}
        "#,
        "match the declared type",
    );
}

/// CLOSED (the gap recorded with B29's landing): a `= Self`-defaulted trait
/// generic (`Add<B = Self>`) resolves to the same TYPE as `Self`, so the
/// declared position was ambiguous and went unchecked — a wrong impl type
/// slipped conformance and only errored at use sites. Types are not interned,
/// so the written `Self` and the written `B` keep distinct type ids;
/// conformance now recovers the spelling from `prepped_type_locals` and
/// substitutes accordingly. Here no `with`-clause argument is given, so `B`
/// takes its `= Self` default and the position promises `Meters`.
#[test]
fn a_self_defaulted_generic_position_with_a_wrong_type_is_rejected() {
    assert_fails_with(
        r#"
        import std::operators::Add;
        struct Meters { value: i32 }
        impl Meters with Add {
            fun add(self, b: str): Meters { self }
        }
        fun main() {}
        "#,
        "match the declared type",
    );
}

/// The other half of the same rule, and the case a naive fix breaks: when the
/// `with` clause DOES supply an argument, a `= Self`-defaulted position promises
/// that argument, not the subject. This is std's shape at `time.vl`
/// (`impl Instant with Add<Duration>`) — substituting `B -> subject` here would
/// false-reject the standard library.
#[test]
fn an_argued_self_defaulted_generic_position_takes_the_argument_not_the_subject() {
    assert_compiles(
        r#"
        import std::operators::Add;
        struct Feet { value: i32 }
        struct Meters { value: i32 }
        impl Meters with Add<Feet> {
            fun add(self, b: Feet): Meters { self }
        }
        fun main() {}
        "#,
    );
}

/// ...and the argument position is genuinely CHECKED under an explicit
/// argument, not merely permissive: the subject is the wrong type there.
#[test]
fn an_argued_self_defaulted_generic_position_rejects_the_subject() {
    assert_fails_with(
        r#"
        import std::operators::Add;
        struct Feet { value: i32 }
        struct Meters { value: i32 }
        impl Meters with Add<Feet> {
            fun add(self, b: Meters): Meters { self }
        }
        fun main() {}
        "#,
        "match the declared type",
    );
}

/// The return position takes the OTHER branch of the same rule: `Add` declares
/// `fun add(self, b: B): Self`, so under `Add<Feet>` the argument is `Feet` and
/// the return is still the subject. Returning the argument is the mistake this
/// pins — the two ambiguous positions must not collapse onto one answer.
#[test]
fn a_self_defaulted_generic_return_stays_the_subject_under_an_explicit_argument() {
    assert_fails_with(
        r#"
        import std::operators::Add;
        struct Feet { value: i32 }
        struct Meters { value: i32 }
        impl Meters with Add<Feet> {
            fun add(self, b: Feet): Feet { Feet { value = 1 } }
        }
        fun main() {}
        "#,
        "match the declared return type",
    );
}

/// The argument-less form still conforms end to end (the 100+ std operator
/// impls are this shape): `B` takes its `= Self` default, so both the argument
/// and the return promise the subject.
#[test]
fn an_argument_less_self_defaulted_generic_impl_still_compiles() {
    assert_compiles(
        r#"
        import std::operators::Add;
        struct Meters { value: i32 }
        impl Meters with Add {
            fun add(self, b: Meters): Meters { self }
        }
        fun main() {}
        "#,
    );
}

/// std's own `time.vl` through the real library, not a reconstruction: `Instant`
/// implements `Add<Duration>` and `Sub<Duration>`, the two explicit-argument
/// sites in std, and both must keep compiling AND running.
#[test]
fn std_instant_arithmetic_conforms_through_the_real_library() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::{ now, Duration };

        fun main() {
            let start = now();
            let later = start + Duration::millis(500i53);
            let back = later - Duration::millis(500i53);
            print(back == start);
        }
        "#,
        "true\n",
    );
}

/// B31 (found by A13 S2a's probes, HMR-independent; fixed): a module-level
/// closure binding referenced *only* by CALL (`f()`) used to be dropped from
/// the emitted globals while the call site remained — the bundle threw
/// `f is not defined` at runtime. Root cause was the assembly-time tree-shake,
/// not reachability: the call-graph walk DID reach the binding (its call
/// subject is a recorded `global_reference`), but the transformer's `Expr::Call`
/// arm reads the `Expr::Local` callee subject directly and emits `f(..)` by
/// name without recording it in `referenced_globals` — so assembly then dropped
/// the declaration. The fix records the reference in that arm, mirroring the
/// value arm's unconditional insert.
#[test]
fn a_module_level_closure_binding_referenced_only_by_call_still_emits_its_declaration() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        let f = || 0;

        fun main() {
            print(i"{f()}");
        }
        "#,
        "0\n",
    );
}

/// B31 edge — same root cause reached through another binding's INITIALIZER: `a`
/// is referenced only by the call inside `b`'s initializer (`let b = a()`).
/// Emitting `b`'s init runs through the same `Expr::Call` arm, so `a` must be
/// recorded and kept, else `b = a()` throws `a is not defined` at load.
#[test]
fn a_module_binding_called_only_inside_another_bindings_initializer_survives() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        let a = || 7;
        let b = a();

        fun main() {
            print(i"{b}");
        }
        "#,
        "7\n",
    );
}

/// B31 edge — TRANSITIVE reachability: `main` calls `b`, whose closure body
/// calls `a`. `b` must be kept (main's call records it) AND `a` must be kept
/// (b's body call records it). Before the fix both were dropped (`b` first,
/// because main's call didn't record it, so its body was never even emitted).
#[test]
fn transitive_module_closure_calls_are_all_kept() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        let a = || 5;
        let b = || a();

        fun main() {
            print(i"{b()}");
        }
        "#,
        "5\n",
    );
}

/// B31 edge — a closure binding declared in a nested `mod`, referenced only by
/// call (`inner::f()`). Module-level bindings include `mod`-scoped `let`s, so
/// the same tree-shake applies; before the fix the declaration was dropped.
#[test]
fn a_nested_mod_closure_binding_referenced_only_by_call_survives() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        mod inner {
            export let f = || 3;
        }

        fun main() {
            print(i"{inner::f()}");
        }
        "#,
        "3\n",
    );
}

/// B31 edge — a module closure whose CALL result is postfixed with the `?`
/// try/lift operator (`g(20)? + g(22)?` in a lift region). The postfix wraps the
/// call, but the callee is still emitted through the same `Expr::Call` arm, so
/// the fix keeps `g`; before it, the emitted `g(..)` threw `g is not defined`.
#[test]
fn a_module_closure_called_through_a_try_region_postfix_survives() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        let g = |n: i32| Some(n);

        fun main() {
            print((g(20)? + g(22)?).unwrap_or(0));
        }
        "#,
        "42\n",
    );
}

/// B31 edge — a module closure whose CALL result is postfixed with the `!`
/// force operator, inside a function that returns `Option`. Same callee-emission
/// path as the try postfix; the fix keeps `g`.
#[test]
fn a_module_closure_called_through_a_force_postfix_survives() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        let g = |n: i32| Some(n);

        fun pick(): Option<i32> {
            let x = g(20)!;
            Some(x)
        }

        fun main() {
            print(pick().unwrap_or(0));
        }
        "#,
        "20\n",
    );
}

/// B31 edge — a module closure called at the head of a `?.` try-and-lift CHAIN
/// (`find(true)?.title`). The lift continuation is a different codegen path from
/// the bare-`?` region above, but the callee `find` is emitted through the same
/// `Expr::Call` arm, so the fix keeps it.
#[test]
fn a_module_closure_called_at_the_head_of_a_try_chain_survives() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Book { title: str }

        let find = |hit: bool| if hit { Some(Book { title = "dune" }) } else { None };

        fun main() {
            print((find(true)?.title).unwrap_or("none"));
        }
        "#,
        "dune\n",
    );
}

/// B31 regression — a module closure passed as a VALUE argument already worked
/// (an argument is walked through `walk_entity`, whose `Expr::Local` value arm
/// records the reference); pinned so the general fix doesn't quietly change the
/// already-good path.
#[test]
fn a_module_closure_passed_as_an_argument_survives() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun apply(g: || i32): i32 {
            g()
        }

        let f = || 9;

        fun main() {
            print(i"{apply(f)}");
        }
        "#,
        "9\n",
    );
}

/// B31 precision guard — the general fix must NOT keep a genuinely-dead binding.
/// `unused_leaf` is never referenced, so its declaration must still be
/// tree-shaken away; `kept_leaf` (called) is retained. The `kept_leaf` assertion
/// makes the check self-validating: module-level names are emitted verbatim, so
/// if a future rename pass mangled them the positive check would fail rather
/// than let the negative one pass vacuously.
#[test]
fn a_genuinely_dead_module_closure_is_still_tree_shaken() {
    let js = compile(
        r#"
        import std::io::print;

        let kept_leaf = || 1;
        let unused_leaf = || 2;

        fun main() {
            print(i"{kept_leaf()}");
        }
        "#,
    )
    .expect("clean compile");
    assert!(
        js.contains("kept_leaf"),
        "the called binding must be emitted; got:\n{js}"
    );
    assert!(
        !js.contains("unused_leaf"),
        "the dead binding must be tree-shaken; got:\n{js}"
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        let kept_leaf = || 1;
        let unused_leaf = || 2;

        fun main() {
            print(i"{kept_leaf()}");
        }
        "#,
        "1\n",
    );
}

/// `stash` inside a generic function is rejected at the lexical call site (the
/// check is not per-instantiation — hmr.md §11 S2's recorded refinement), and
/// the diagnostic must name the unbounded-generic cause rather than accuse the
/// value: there is no bound the author could add to make it compile.
#[test]
fn hmr_stash_in_a_generic_function_names_the_unbounded_generic_cause() {
    assert_fails_browser_with(
        r#"
        import std::dev;

        fun relay<type T>(key: str, value: T) {
            dev::stash(key, value);
        }

        fun main() {
            relay("count", 3);
        }
        "#,
        "is a generic type parameter here",
    );
}

// --- B32: an unknown value name is unresolved, not void, so it never cascades

#[test]
fn an_unknown_value_name_reports_once_without_type_cascade() {
    // B32 (found by E7's cascade probes): an unknown name used as a VALUE
    // used to type as `void`, so the one root error ("cannot find …")
    // cascaded into `Expected i32, but got void` at the annotated binding AND
    // at the call argument. The fix types `Expr::Error` as `Type::Unresolved`
    // (the non-cascading answer the unresolved-*call* path already flows
    // through), so both downstream positions stay silent.
    let diagnostics = failure_diagnostics(
        r#"
        fun print_field(value: i32) {}

        fun main() {
            let a = zzz_missing;
            let b: i32 = a;
            print_field(a);
        }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "the unknown name must report once, with no void-typed cascade: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].0.contains("cannot find 'zzz_missing'"),
        "the lone diagnostic must be the root, not a downstream echo: {diagnostics:#?}"
    );
}

#[test]
fn an_unknown_value_stays_silent_at_every_downstream_position() {
    // The bare-NAME twin of the E7 multi-use pin (which used a
    // `zzz_missing(1)` CALL): one unknown name feeds a plain variable, a
    // field access, a call argument, a struct field, and a match subject.
    // Every one of those used to fan a `void` type error (field access even
    // reported `cannot access field … on type void`); now the poison is
    // `Unresolved`, so each position defers and is demoted behind the root.
    // Exactly ONE diagnostic — the root — survives.
    let diagnostics = failure_diagnostics(
        r#"
        struct Box { v: i32 }
        fun take(x: i32): i32 { x }
        fun main() {
            let root = zzz_missing;
            let via_var = root;
            let via_field = root.field;
            let via_call = take(root);
            let via_struct = Box { v = root };
            let via_match = match root {
                _ => 1,
            };
        }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "no downstream position may echo the poison: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].0.contains("cannot find 'zzz_missing'"),
        "the lone diagnostic must be the root: {diagnostics:#?}"
    );
    // Belt and braces: not one of the void/unknown-typed cascade shapes.
    assert!(
        diagnostics
            .iter()
            .all(|(message, _)| !message.contains("but got void")
                && !message.contains("on type void")
                && !message.contains("on type unknown")
                && !message.contains("could not be resolved")),
        "no void/unknown/residual cascade may survive: {diagnostics:#?}"
    );
}

#[test]
fn two_independent_unknown_names_each_report_their_own_root() {
    // Ripple (a): the poison must not swallow a DIFFERENT genuine error
    // downstream of it. `a`'s value is unknown, and a separate unknown name
    // sits in an argument position — both roots must stand.
    let diagnostics = failure_diagnostics(
        r#"
        fun foo(x: i32) {}
        fun main() {
            let a = zzz_missing;
            foo(b_also_missing);
        }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        2,
        "both independent roots must report: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("cannot find 'zzz_missing'")),
        "the first root must stand: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("cannot find 'b_also_missing'")),
        "the second, independent root must stand: {diagnostics:#?}"
    );
}

#[test]
fn an_unknown_value_does_not_poison_a_sibling_binding() {
    // Ripple (b): the poison must not spread through unification into a
    // sibling's constraints. `b`/`c` are wholly unrelated to `a` and must
    // both type and stay clean — only the root survives.
    let clean = failure_diagnostics(
        r#"
        fun main() {
            let a = zzz_missing;
            let b: i32 = 5;
            let c: i32 = b + 1;
        }
        "#,
    );
    assert_eq!(
        clean.len(),
        1,
        "an unrelated sibling must not inherit the poison: {clean:#?}"
    );
    assert!(
        clean[0].0.contains("cannot find 'zzz_missing'"),
        "the lone diagnostic must be the root: {clean:#?}"
    );

    // And the sibling's inference is genuinely LIVE, not merely silenced: a
    // real type error on `b` still fires (alongside the untouched root).
    let live = failure_diagnostics(
        r#"
        fun main() {
            let a = zzz_missing;
            let b: i32 = 5;
            let d: str = b;
        }
        "#,
    );
    assert!(
        live.iter()
            .any(|(message, _)| message.contains("cannot find 'zzz_missing'")),
        "the root must stand: {live:#?}"
    );
    assert!(
        live.iter()
            .any(|(message, _)| message.contains("Expected str, but got i32")),
        "the sibling's own type error must still fire — inference is live: {live:#?}"
    );
}

#[test]
fn an_unknown_value_as_a_generic_argument_does_not_ghost_report() {
    // Ripple (c): passing the poison to a generic must not panic or ghost-
    // report (a spurious binding error), and a well-typed instantiation of
    // the same generic beside it stays clean. Only the root survives.
    let diagnostics = failure_diagnostics(
        r#"
        fun identity<type T>(value: T): T { value }
        fun main() {
            let a = zzz_missing;
            let r = identity(a);
            let s: str = identity("ok");
        }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "a poisoned generic argument must not ghost-report: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].0.contains("cannot find 'zzz_missing'"),
        "the lone diagnostic must be the root: {diagnostics:#?}"
    );
}

#[test]
fn a_closure_capturing_an_unknown_value_reports_only_its_own_errors() {
    // Ripple (d): a closure that captures the poison must stay silent about
    // IT, but must still report the closure's OWN independent error. Two
    // roots, no cascade from the captured poison.
    let diagnostics = failure_diagnostics(
        r#"
        fun main() {
            let a = zzz_missing;
            let g = |x: i32| x + a;
            let h = |x: i32| x + other_missing;
        }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        2,
        "the captured poison stays silent; the closure's own error fires: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("cannot find 'zzz_missing'")),
        "the captured-value root must stand: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("cannot find 'other_missing'")),
        "the closure's own root must stand: {diagnostics:#?}"
    );
}

#[test]
fn an_unknown_call_result_reports_once_without_type_cascade() {
    // The unknown-CALL leg (E7's already-clean path) must not regress under
    // the B32 fix — and in fact improves: `zzz_missing(1)` is unresolved, so
    // the annotated binding and the call argument stay silent, and the
    // call-subject cascade (`cannot call … it is void`) that used to accompany
    // the root is gone too. One diagnostic, the root.
    let diagnostics = failure_diagnostics(
        r#"
        fun print_field(value: i32) {}

        fun main() {
            let a = zzz_missing(1);
            let b: i32 = a;
            print_field(a);
        }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "the unknown-call result must not cascade a void type error: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].0.contains("cannot find 'zzz_missing'"),
        "the lone diagnostic must be the root: {diagnostics:#?}"
    );
}

#[test]
fn a_genuine_non_function_call_still_reports_its_type() {
    // Guard the precedent the B32 fix must NOT disturb: calling a real
    // non-function value (`42`, an `i32` — not an `Expr::Error`) still names
    // the subject's type. Only `Expr::Error` became `Unresolved`; a concrete
    // non-callable type is unaffected.
    assert_fails_with(
        r#"
        fun main() {
            let x = (42)(1);
        }
        "#,
        "cannot call this as a function: it is i32",
    );
}

// --- Server-side rendering: the process-layer `std::ui` (A7, proposal/ssr.md) --
//
// On `@process` (the default platform here) `std::ui` builds an HTML string tree
// and `render` serializes it. Each pin is one binding form rendered to an exact
// string: attributes in insertion order, escaping in text and attribute values,
// void elements without a closing tag, read-once bindings, discarded handlers.

#[test]
fn ssr_renders_static_view_with_ordered_attributes_and_nesting() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("div").class("card").attr("data-id", "7").child(view("p").text("hi"))));
        }
        "#,
        "<div class=\"card\" data-id=\"7\"><p>hi</p></div>\n",
    );
}

#[test]
fn ssr_svg_root_carries_its_namespace() {
    // B37: the process twin seeds `xmlns` on an `svg` root (descendants
    // inherit), before the component's own attributes; a component setting
    // `xmlns` itself replaces the seed in place.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("svg")
                .attr("viewBox", "0 0 24 24")
                .child(view("path").attr("d", "M5 12h14"))));
            print(render(view("svg").attr("xmlns", "urn:custom")));
        }
        "#,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M5 12h14\"></path></svg>\n<svg xmlns=\"urn:custom\"></svg>\n",
    );
}

#[test]
fn browser_view_routes_svg_tags_through_create_element_ns() {
    // B37's browser half, pinned at the codegen level: an svg-family tag
    // creates through `createElementNS` (an HTML-namespace `<svg>` renders
    // nothing), a plain tag through `createElement`, and the ambiguous tags
    // (`a`, `title`, `style`, `script`) stay HTML. Boundary-less on purpose:
    // static slots resolve to impls that read no context, so no owner is
    // demanded (the H8 per-instantiation coverage fix restored this shape).
    let js = compile_browser(
        r#"
        import std::ui::{ view, View };
        fun main() {
            let _icon = view("svg").child(view("path").attr("d", "M5 12h14"));
            let _link = view("div").child(view("a").attr("href", "/"));
        }
        main();
        "#,
    )
    .expect("a clean browser compile");
    assert!(
        js.contains("document.createElementNS"),
        "svg tags must route through createElementNS:\n{js}"
    );
    assert!(
        js.contains("\"http://www.w3.org/2000/svg\""),
        "the SVG namespace constant must be emitted:\n{js}"
    );
    assert!(
        js.contains("document.createElement"),
        "plain tags still route through createElement:\n{js}"
    );
}

#[test]
fn ssr_bind_text_embeds_current_signal_value() {
    // Read-once: `bind_text` takes `signal.get()` at render time — no subscription.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("h1").bind_text(Signal::new("world"))));
        }
        "#,
        "<h1>world</h1>\n",
    );
}

#[test]
fn ssr_bind_class_and_bind_attr_read_once() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("a").bind_class(Signal::new("active")).bind_attr("href", Signal::new("/x")).text("go")));
        }
        "#,
        "<a class=\"active\" href=\"/x\">go</a>\n",
    );
}

/// `bind_styled` is `styled`'s reactive twin, so the process twin reads the
/// signal once and renders the style it held — the class names being the
/// content hashes the `const` chain already emitted.
#[test]
fn ssr_bind_styled_reads_the_current_style_once() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::style::{ style, space, Style };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            let compact = const style().padding(space(2));
            let roomy = const style().padding(space(6));
            let theme: SignalCell<Style> = Signal::new(compact);
            print(render(view("div").bind_styled(theme)));
            theme.set(roomy);
            print(render(view("div").bind_styled(theme)));
        }
        "#,
        "<div class=\"s1ufvp8\"></div>\n<div class=\"s1ufvsw\"></div>\n",
    );
}

/// The construct-in-const rule survives a signal in the middle: a `SignalCell<Style>`
/// can only ever carry styles some `const` expression already emitted, so
/// building one at the binding site is still the static error it always was.
#[test]
fn bind_styled_cannot_construct_its_style_at_runtime() {
    assert_fails_with(
        r#"
        import std::ui::{ view, View, render };
        import std::style::{ style, space, Style };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            let theme: SignalCell<Style> = Signal::new(style().padding(space(2)));
            print(render(view("div").bind_styled(theme)));
        }
        main();
        "#,
        "compile-time-only",
    );
}

#[test]
fn ssr_bind_each_renders_current_list() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            let items: SignalCell<List<str>> = Signal::new(["a", "b", "c"]);
            print(render(view("ul").bind_each(items, |s| s, |s| view("li").text(s))));
        }
        "#,
        "<ul><li>a</li><li>b</li><li>c</li></ul>\n",
    );
}

#[test]
fn ssr_bind_each_over_empty_list_renders_no_rows() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            let items: SignalCell<List<str>> = Signal::new([]);
            print(render(view("ul").bind_each(items, |s| s, |s| view("li").text(s))));
        }
        "#,
        "<ul></ul>\n",
    );
}

#[test]
fn ssr_when_renders_the_taken_branch_only() {
    // Both branches: true renders the body, false renders nothing.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("div").when(Signal::new(true), || view("p").text("shown"))));
            print(render(view("div").when(Signal::new(false), || view("p").text("shown"))));
        }
        "#,
        "<div><p>shown</p></div>\n<div></div>\n",
    );
}

#[test]
fn ssr_swap_renders_the_current_value_branch() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        [derive(PartialEq)]
        enum Tab { A, B }
        fun main() {
            print(render(view("nav").swap(Signal::new(Tab::B), |t| match t {
                Tab::A => view("a").text("first"),
                Tab::B => view("a").text("second"),
            })));
        }
        "#,
        "<nav><a>second</a></nav>\n",
    );
}

#[test]
fn ssr_show_toggles_the_hidden_attribute() {
    // `show(true)` renders nothing extra; `show(false)` adds `hidden` (mirrors the
    // DOM's `element.hidden`).
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("span").show(Signal::new(true))));
            print(render(view("span").show(Signal::new(false))));
        }
        "#,
        "<span></span>\n<span hidden=\"\"></span>\n",
    );
}

#[test]
fn ssr_style_var_folds_into_the_style_attribute() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("div").style_var("--w", Signal::new("40px")).style_var("--h", Signal::new("10px"))));
        }
        "#,
        "<div style=\"--w:40px;--h:10px\"></div>\n",
    );
}

#[test]
fn ssr_bind_value_renders_the_input_value() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("input").attr("type", "text").bind_value(Signal::new("hello"))));
        }
        "#,
        "<input type=\"text\" value=\"hello\">\n",
    );
}

#[test]
fn ssr_bind_draft_renders_the_local_value() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell, draft, Draft };
        import std::option::Option::{ self, Some, None };
        import std::io::print;
        fun main() {
            let name = draft("initial", |value: str| {
                let _ignore = value;
                None
            });
            print(render(view("input").bind_draft(name)));
        }
        "#,
        "<input value=\"initial\">\n",
    );
}

#[test]
fn ssr_children_appends_all_views() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("ul").children([view("li").text("x"), view("li").text("y")])));
        }
        "#,
        "<ul><li>x</li><li>y</li></ul>\n",
    );
}

#[test]
fn ssr_escapes_text_nodes() {
    // A hostile string renders inert: `&`, `<`, `>` become entities. The quote is
    // NOT escaped in a text node (only attribute values need that).
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("p").text("<script>alert(\"&\")</script>")));
        }
        "#,
        "<p>&lt;script&gt;alert(\"&amp;\")&lt;/script&gt;</p>\n",
    );
}

#[test]
fn ssr_escapes_attribute_values() {
    // Attribute values escape `&` and `"` (the double-quote delimiter); `<`/`>` are
    // legal inside a quoted attribute and stay literal.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("a").attr("title", "a \"b\" & <c>")));
        }
        "#,
        "<a title=\"a &quot;b&quot; &amp; <c>\"></a>\n",
    );
}

#[test]
fn ssr_void_elements_have_no_closing_tag() {
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("br")));
            print(render(view("img").attr("src", "/x.png")));
            print(render(view("hr")));
        }
        "#,
        "<br>\n<img src=\"/x.png\">\n<hr>\n",
    );
}

#[test]
fn ssr_void_element_drops_children() {
    // Children of a void element are illegal HTML — a documented no-op (they are
    // simply not serialized), not a build error.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("br").child(view("span").text("nope"))));
        }
        "#,
        "<br>\n",
    );
}

#[test]
fn ssr_event_handler_is_discarded_and_never_runs() {
    // A server-rendered button is just a button: `on` accepts the handler and
    // drops it. The handler's side effect (a `print`) never fires, so stdout is the
    // markup alone — an extra line would appear if the closure ran.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("button").text("click me").on("click", || print("HANDLER RAN"))));
        }
        "#,
        "<button>click me</button>\n",
    );
}

#[test]
fn ssr_text_replaces_children() {
    // `text` mirrors the DOM's `textContent`: it replaces any children the node had.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("div").child(view("span").text("old")).text("new")));
        }
        "#,
        "<div>new</div>\n",
    );
}

#[test]
fn ssr_nested_component_composition() {
    // A "component" is a function returning a `View`; composition is just calls.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun badge(label: str): View {
            view("span").class("badge").text(label)
        }
        fun main() {
            print(render(view("div").child(badge("new")).child(badge("hot"))));
        }
        "#,
        "<div><span class=\"badge\">new</span><span class=\"badge\">hot</span></div>\n",
    );
}

#[test]
fn ssr_child_interleaves_text_and_element_children() {
    // Element-syntax S1 (proposal/element-syntax.md §5): `child` is
    // `Slot`-typed — a `str` child is a TEXT NODE, a sibling of element
    // children, in written order, escaped like any text.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("p")
                .child("Take ")
                .child(view("code").text("vilan upgrade"))
                .child(" & <go>")));
        }
        "#,
        "<p>Take <code>vilan upgrade</code> &amp; &lt;go&gt;</p>\n",
    );
}

#[test]
fn ssr_child_reads_a_signal_text_node_once() {
    // The `SignalCell<str>` arm of `Slot`, read once — the value at render time is
    // the value served, escaped as text.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("p").child("now: ").child(Signal::new("a & b"))));
        }
        "#,
        "<p>now: a &amp; b</p>\n",
    );
}

#[test]
fn ssr_child_accepts_a_list_of_views() {
    // The `List<View>` arm of `Slot`: one child position, every view, in order.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            let pair: List<View> = [view("i").text("a"), view("b").text("b")];
            print(render(view("p").child(pair)));
        }
        "#,
        "<p><i>a</i><b>b</b></p>\n",
    );
}

#[test]
fn ssr_attr_reads_a_signal_value_once() {
    // The `SignalCell<str>` arm of `AttrValue` — `attr` with a signal is exactly
    // `bind_attr`: read once here, tracked on the browser twin.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun main() {
            print(render(view("a").attr("href", Signal::new("/x")).text("go")));
        }
        "#,
        "<a href=\"/x\">go</a>\n",
    );
}

#[test]
fn ssr_text_replaces_text_node_children_too() {
    // `text`'s replace-children semantics reach the new text nodes: it clears
    // them exactly as it clears element children.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("p").child("gone").text("kept")));
        }
        "#,
        "<p>kept</p>\n",
    );
}

#[test]
fn browser_text_children_ride_create_text_node() {
    // The browser twin's `str` and `SignalCell<str>` `Slot` arms append real text
    // nodes (`document.createTextNode`) — siblings of element children, never
    // wrapper spans. The signal arm re-sets the node's own text on change.
    let js = compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell };
        import std::ui::{ mount_root, view };
        fun main() {
            mount_root("app", || {
                let status = Signal::new("ready");
                view("p").child("state: ").child(status).child(view("b").text("!"))
            });
        }
        main();
        "#,
    )
    .expect("a clean browser compile");
    assert!(
        js.contains("document.createTextNode"),
        "text children must create text nodes:\n{js}"
    );
}

#[test]
fn element_lowering_is_the_chain_byte_for_byte() {
    // Element-syntax §4's contract at the strongest level: the desugar builds
    // the very trees the chain parses to, so the emitted JS is byte-identical.
    let element = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        import std::ui::{ View, render, view };
        fun main() {
            let name = Signal::new("world");
            let page = <p data-live(name) title("hi")>
                "Take "
                <code>"vilan upgrade"</code>
                {name}
            </p>;
            print(render(page));
        }
        "#;
    let chain = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        import std::ui::{ View, render, view };
        fun main() {
            let name = Signal::new("world");
            let page = view("p").attr("data-live", name).attr("title", "hi")
                .child("Take ")
                .child(view("code").child("vilan upgrade"))
                .child(name);
            print(render(page));
        }
        "#;
    assert_eq!(
        compile(element).expect("the element program compiles"),
        compile(chain).expect("the chain program compiles"),
        "the element lowering must emit the chain's exact JS"
    );
}

#[test]
fn element_event_arity_lowers_to_on_and_on_event_byte_for_byte() {
    // The browser leg, and the `on:` table rows: a zero-parameter literal is
    // `.on`, a one-parameter literal is `.on_event` — byte-identical to the
    // chain spellings.
    let element = r#"
        import std::ui::{ View, mount_root, view };
        fun main() {
            mount_root("app", || {
                view("div")
                    .child(<button on:click(|| beep())>"go"</button>)
                    .child(<a on:click(|e| e.prevent_default())>"stay"</a>)
            });
        }
        fun beep() {}
        main();
        "#;
    let chain = r#"
        import std::ui::{ View, mount_root, view };
        fun main() {
            mount_root("app", || {
                view("div")
                    .child(view("button").on("click", || beep()).child("go"))
                    .child(view("a").on_event("click", |e| e.prevent_default()).child("stay"))
            });
        }
        fun beep() {}
        main();
        "#;
    assert_eq!(
        compile_browser(element).expect("the element program compiles"),
        compile_browser(chain).expect("the chain program compiles"),
        "the element event lowering must emit the chain's exact JS"
    );
}

#[test]
fn ssr_element_renders_mixed_content() {
    // An element program end to end on the process twin: written-order
    // attributes, escaped text children, a nested element.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::ui::{ View, render, view };
        fun main() {
            print(render(<p title("a & b")>
                "Take "
                <code>"vilan upgrade"</code>
                " & <go>"
            </p>));
        }
        "#,
        "<p title=\"a &amp; b\">Take <code>vilan upgrade</code> &amp; &lt;go&gt;</p>\n",
    );
}

#[test]
fn hyphenated_attribute_names_parse_and_emit_verbatim() {
    // E87 (element-syntax.md §2, blessed 2026-08-22): hyphens are ordinary
    // attribute-name characters, exactly as in HTML — `data-*`/`aria-*` need
    // no special form and no method twin, because the name-blind desugar
    // lowers `data-foo-bar("x")` to `.attr("data-foo-bar", "x")` without
    // ever reading the name. The owner's probe, pinned end to end: parse,
    // check, and the emitted attributes verbatim.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::ui::{ View, render, view };
        fun main() {
            print(render(<div data-foo-bar("x") aria-label("y")>"z"</div>));
        }
        "#,
        "<div data-foo-bar=\"x\" aria-label=\"y\">z</div>\n",
    );
}

#[test]
fn an_element_without_view_in_scope_fails_at_the_element_head() {
    // No auto-import: the desugared `view` accessor spans `<tag`, so the
    // unresolved-name diagnostic underlines the element head the user wrote —
    // and carries the import steer as a note (element-syntax S4).
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = <div/>;
        }
        "#,
        "<div",
        "cannot find 'view' in this scope",
    );
    assert_fails_noting(
        r#"
        fun main() {
            let _x = <div/>;
        }
        "#,
        "cannot find 'view' in this scope",
        "<div",
        "element syntax lowers to std::ui::view",
    );
}

// --- A35: the element desugar's `view`, SHADOWED --------------------------
//
// The desugar's callee is a bare `view`, so a user item of that name captures
// it. lucide ships an icon called `view`, its generated `fun view(): View`
// silently took over, and every `<tag />` in the file reported
// `` `view` expects 0 arguments, but got 1 instead `` against the ELEMENT — an
// arity nobody wrote, with the shadowing item shown only as "declared here".
// RULED 2026-09-01: name it in the diagnostic. The desugar stays capturable
// (shadowing a name is a ruled feature; `an_explicit_import_shadows_a_prelude_
// name_silently` is the neighbouring pin), so this is a message, not hygiene.

#[test]
fn a35_a_shadowed_element_view_names_the_shadow_instead_of_an_arity() {
    assert_fails_spanning(
        r#"
        fun view(): i32 { 1 }
        fun main() {
            let _x = <div/>;
        }
        "#,
        "div",
        "element syntax lowers to `std::ui::view`, and `view` here is your own `fun view`",
    );
}

#[test]
fn a35_the_shadowed_steer_names_both_ways_out() {
    // Renaming, or writing the element as the call it lowers to. Neither is
    // guessable from `` `view` expects 0 arguments ``, which is why the
    // curated message exists at all.
    assert_fails_with(
        r#"
        fun view(): i32 { 1 }
        fun main() {
            let _x = <div/>;
        }
        "#,
        "rename it, or write this element as its lowered call, `ui::view(…)`",
    );
    // Both ways out, compiled (audit run 7's steer sweep, ledger row 360).
    // The lowered call, which needs the module rather than the bare name:
    assert_compiles(
        r#"
        import std::ui;
        fun view(): i32 { 1 }
        fun main() {
            let _x = ui::view("div");
        }
        "#,
    );
    // And the rename. What the element lowers to has to still be REACHABLE
    // after the shadow is gone — ambient from the web prelude in the projects
    // that meet this message, and named explicitly here, since the harness
    // compiles against the base one.
    assert_compiles(
        r#"
        import std::ui::view;
        fun render(): i32 { 1 }
        fun main() {
            let _x = <div/>;
        }
        "#,
    );
}

#[test]
fn a35_the_shadowed_message_still_points_at_the_declaration() {
    // The C3 note is what makes "your own `fun view`" actionable: it is the
    // site. It was already there; the primary is what changed.
    assert_fails_noting(
        r#"
        fun view(): i32 { 1 }
        fun main() {
            let _x = <div/>;
        }
        "#,
        "element syntax lowers to `std::ui::view`",
        // The first `view` in the source is the declaration's own name span,
        // which is where the C3 note lands.
        "view",
        "`view` is declared here",
    );
}

#[test]
fn a35_a_hand_written_view_call_keeps_the_ordinary_arity_message() {
    // The control the detection rests on: element origin is the SUBJECT's
    // markup span (it starts with `<`), so a `view(..)` the author actually
    // typed — whose subject span is the ident — is an ordinary arity mistake
    // and must not be told about element syntax it never used.
    assert_fails_with(
        r#"
        fun view(): i32 { 1 }
        fun main() {
            let _x = view(1);
        }
        "#,
        "`view` expects 0 arguments, but got 1 instead",
    );
    assert_fails_without(
        r#"
        fun view(): i32 { 1 }
        fun main() {
            let _x = view(1);
        }
        "#,
        "element syntax lowers to",
    );
}

#[test]
fn a35_the_absent_case_still_gets_the_import_steer() {
    // The two arms are twins and must not collapse into one: with no `view` in
    // scope at all there is nothing to name as a shadow, and the answer is the
    // import.
    assert_fails_noting(
        r#"
        fun main() {
            let _x = <div/>;
        }
        "#,
        "cannot find 'view' in this scope",
        "<div",
        "element syntax lowers to std::ui::view; add",
    );
}

#[test]
fn an_element_text_attribute_warns_toward_the_content_method() {
    // Element-syntax S4: `text(…)` undotted in a head is an attribute — the
    // one str-typed method name the type system cannot catch. The warning
    // fires on the element form only (the lowered name argument's span is the
    // UNQUOTED attribute name).
    let messages = warnings(
        r#"
        import std::ui::{ View, view };
        fun main() {
            let _x = <div text("hi") />;
        }
        "#,
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("`text(…)` in an element head is an attribute")),
        "expected the element text-attribute warning, got {messages:?}"
    );
}

#[test]
fn a_hand_written_text_attr_does_not_warn() {
    let messages = warnings(
        r#"
        import std::ui::{ View, view };
        fun main() {
            let _x = view("div").attr("text", "hi");
        }
        "#,
    );
    assert!(
        messages.is_empty(),
        "expected no warnings, got {messages:?}"
    );
}

#[test]
fn a_macro_generated_element_desugars() {
    // parse_generated runs the element desugar too — markup emitted by a
    // macro lowers like hand-written markup.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::ui::{ View, render, view };
        fun main() {
            let banner = macro {
                import macro_std::source;
                source("<p>\"from a macro\"</p>")
            };
            print(render(banner));
        }
        "#,
        "<p>from a macro</p>\n",
    );
}

#[test]
fn a_mismatched_closing_tag_names_the_expected_close() {
    assert_fails_with(
        r#"
        import std::ui::{ View, view };
        fun main() {
            let _x = <div>"x"</span>;
        }
        "#,
        "</div>",
    );
}

#[test]
fn a_generic_method_dispatches_a_bound_on_a_closure_parameter() {
    // The silent-stub misrender (found by element-syntax S2's probe, general
    // and pre-existing): a bound-generic METHOD call whose argument is an
    // unannotated closure parameter resolved prematurely — the param was
    // still Unknown, nothing was recorded in `method_call_substitution`, and
    // the transformer monomorphized to the trait's empty abstract member.
    // The method path now defers like the free-function path and retries
    // once the closure's owning call lands the type.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Speak {
            fun speak(self): str;
        }
        struct Dog {
            name: str,
        }
        impl Dog with Speak {
            fun speak(self): str {
                "arf"
            }
        }
        struct Kennel {
            log: str,
        }
        impl Kennel {
            fun hold<C: Speak>(self, guest: C): Kennel {
                Kennel { log = self.log + guest.speak() }
            }
        }
        fun apply(dog: Dog, visit: |Dog| Kennel): Kennel {
            visit(dog)
        }
        fun main() {
            let direct = Kennel { log = "" }.hold(Dog { name = "rex" });
            print(direct.log);
            let via = apply(Dog { name = "rex" }, |d| Kennel { log = "" }.hold(d));
            print(via.log);
        }
        "#,
        "arf\narf\n",
    );
}

#[test]
fn bind_each_rows_dispatch_slot_children() {
    // The same bug's std face, the one every real app hits: a row closure's
    // `.child(t)` dropped the text (empty stub) while a literal child worked.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        import std::ui::{ View, render, view };
        fun main() {
            let items: SignalCell<List<str>> = Signal::new(["alpha", "beta"]);
            print(render(view("ul").bind_each(items, |t| t, |t| view("li").child(t))));
        }
        "#,
        "<ul><li>alpha</li><li>beta</li></ul>\n",
    );
}

#[test]
fn a_closure_parameter_of_an_unimplemented_type_fails_the_bound() {
    // The diagnostic hole the stub opened: with the param typed through the
    // owning call, the bound audit must reject an argument type with no impl
    // — this COMPILED CLEANLY and misrendered before the fix.
    assert_fails_with(
        r#"
        trait Speak {
            fun speak(self): str;
        }
        struct Kennel {
            log: str,
        }
        impl Kennel {
            fun hold<C: Speak>(self, guest: C): Kennel {
                Kennel { log = self.log + guest.speak() }
            }
        }
        fun apply(n: i32, visit: |i32| Kennel): Kennel {
            visit(n)
        }
        fun main() {
            let _via = apply(5, |n| Kennel { log = "" }.hold(n));
        }
        "#,
        "'i32' does not implement trait 'Speak'",
    );
}

#[test]
fn a_let_bound_closure_with_an_untypable_parameter_reports_honestly() {
    // The one shape the deferral cannot finish: a let-bound closure whose
    // parameter no owning call ever types. Before the fix this misrendered
    // silently; now it is an honest unresolved-type diagnostic (annotating
    // the parameter resolves it).
    assert_fails_with(
        r#"
        import std::ui::{ View, render, view };
        fun main() {
            let wrap = |x| view("p").child(x);
            let _page = render(wrap("later"));
        }
        "#,
        "could not be resolved",
    );
}

#[test]
fn an_annotated_closure_parameter_dispatches_directly() {
    // Regression fence: an ANNOTATED closure param was never broken (the body
    // types immediately) — it must stay direct.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Speak {
            fun speak(self): str;
        }
        struct Dog {
            name: str,
        }
        impl Dog with Speak {
            fun speak(self): str {
                "arf"
            }
        }
        struct Kennel {
            log: str,
        }
        impl Kennel {
            fun hold<C: Speak>(self, guest: C): Kennel {
                Kennel { log = self.log + guest.speak() }
            }
        }
        fun apply(dog: Dog, visit: |Dog| Kennel): Kennel {
            visit(dog)
        }
        fun main() {
            let via = apply(Dog { name = "rex" }, |d: Dog| Kennel { log = "" }.hold(d));
            print(via.log);
        }
        "#,
        "arf\n",
    );
}

#[test]
fn a_generic_free_function_dispatches_a_bound_on_a_closure_parameter() {
    // Regression fence: the free-function path always had the fill-or-defer
    // rule (`resolve_call_subject`); the method fix must not disturb it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Speak {
            fun speak(self): str;
        }
        struct Dog {
            name: str,
        }
        impl Dog with Speak {
            fun speak(self): str {
                "arf"
            }
        }
        fun greet<C: Speak>(guest: C): str {
            guest.speak()
        }
        fun apply(dog: Dog, visit: |Dog| str): str {
            visit(dog)
        }
        fun main() {
            print(apply(Dog { name = "rex" }, |d| greet(d)));
        }
        "#,
        "arf\n",
    );
}

#[test]
fn browser_static_child_outside_a_boundary_compiles() {
    // The H8 residual, closed: context coverage follows the RESOLVED
    // instantiation where a call records one. `C = View`/`str` select impls
    // that read no context, so the attach-only `mount()` pattern — the
    // playground's styles.vl shape, the v0.21.0 deploy casualty — works
    // again. The Signal arms keep the fence (the guards below).
    compile_browser(
        r#"
        import std::ui::{ mount, view, View };
        fun main() {
            let content = view("div")
                .attr("id", "card")
                .child(view("h2").text("Styled"))
                .child("static text");
            mount("app", content);
        }
        main();
        "#,
    )
    .expect("static slots need no boundary");
}

#[test]
fn a_signal_child_outside_a_boundary_stays_fenced() {
    // The reactive arm subscribes — `C = SignalCell<str>` selects the impl whose
    // `place` reaches the strict owner read, so the fence must hold.
    let errors = compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell };
        import std::ui::{ mount, view, View };
        fun main() {
            mount("app", view("p").child(Signal::new("live")));
        }
        main();
        "#,
    )
    .expect_err("the Signal child arm must stay fenced");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn a_signal_attr_outside_a_boundary_stays_fenced() {
    let errors = compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell };
        import std::ui::{ mount, view, View };
        fun main() {
            mount("app", view("p").attr("data-live", Signal::new("v")));
        }
        main();
        "#,
    )
    .expect_err("the Signal attr arm must stay fenced");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn a_generic_slot_forwarder_with_a_static_slot_compiles() {
    // Requirement polymorphism: the inner call binds `C` to `wrap`'s own
    // generic, and the walk resolves `T` at `wrap`'s call sites — a static
    // instantiation selects only the `str` arm, which reads nothing, so no
    // boundary is demanded (requirement-polymorphism.md §3; flipped from the
    // S1-era conservative pin, proven red against the union fallback).
    compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        fun wrap<T: Slot>(content: T): View {
            view("p").child(content)
        }
        fun main() {
            mount("app", wrap("static"));
        }
        main();
        "#,
    )
    .expect("a static slot through a forwarder needs no boundary");
}

#[test]
fn a_signal_through_a_generic_forwarder_stays_fenced() {
    // The walk must not loosen the reactive arm: the same forwarder with a
    // Signal instantiation selects the subscribing impl, entered from an
    // uncovered top-level call — fenced.
    let errors = compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        import std::reactive::{ Signal, SignalCell };
        fun wrap<T: Slot>(content: T): View {
            view("p").child(content)
        }
        fun main() {
            mount("app", wrap(Signal::new("live")));
        }
        main();
        "#,
    )
    .expect_err("a Signal through a forwarder keeps the fence");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn mixed_forwarder_call_sites_fence_by_their_own_instantiation() {
    // Edge attribution is the outermost resolving caller, not the forwarder:
    // an uncovered STATIC call through `wrap` must not poison a covered
    // SIGNAL call through the same `wrap`. Attaching edges to the forwarder
    // itself would conflate the two paths and reject this program.
    compile_browser(
        r#"
        import std::ui::{ Slot, mount, mount_root, view, View };
        import std::reactive::{ Signal, SignalCell };
        fun wrap<T: Slot>(content: T): View {
            view("p").child(content)
        }
        fun main() {
            mount("app", wrap("static"));
            mount_root("live", || wrap(Signal::new("ticking")));
        }
        main();
        "#,
    )
    .expect("each call site fences by its own instantiation");
}

#[test]
fn a_two_level_forwarder_resolves_through_both_levels() {
    // The walk recurses: `outer` binds `wrap`'s `T` to its own `U`, and `U`
    // resolves at `outer`'s call sites.
    compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        fun wrap<T: Slot>(content: T): View {
            view("p").child(content)
        }
        fun outer<U: Slot>(content: U): View {
            wrap(content)
        }
        fun main() {
            mount("app", outer("static"));
        }
        main();
        "#,
    )
    .expect("two forwarding levels resolve to the static arm");
}

#[test]
fn a_two_level_forwarder_keeps_the_fence_for_a_signal() {
    let errors = compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        import std::reactive::{ Signal, SignalCell };
        fun wrap<T: Slot>(content: T): View {
            view("p").child(content)
        }
        fun outer<U: Slot>(content: U): View {
            wrap(content)
        }
        fun main() {
            mount("app", outer(Signal::new("live")));
        }
        main();
        "#,
    )
    .expect_err("a Signal through two forwarding levels keeps the fence");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn a_self_recursive_forwarder_resolves_exactly() {
    // The visited-skip is exact, not a fallback: the self-call re-derives
    // the same (function, constraint) pair, and every real path enters
    // through the external call, which resolves statically.
    compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        fun wrap<T: Slot>(content: T, depth: i32): View {
            if depth > 0 {
                wrap(content, depth - 1)
            } else {
                view("p").child(content)
            }
        }
        fun main() {
            mount("app", wrap("static", 3));
        }
        main();
        "#,
    )
    .expect("self-recursion resolves through the external entry");
}

#[test]
fn a_self_recursive_forwarder_keeps_the_fence_for_a_signal() {
    let errors = compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        import std::reactive::{ Signal, SignalCell };
        fun wrap<T: Slot>(content: T, depth: i32): View {
            if depth > 0 {
                wrap(content, depth - 1)
            } else {
                view("p").child(content)
            }
        }
        fun main() {
            mount("app", wrap(Signal::new("live"), 3));
        }
        main();
        "#,
    )
    .expect_err("a Signal through a recursive forwarder keeps the fence");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn explicit_type_arguments_resolve_the_forwarder() {
    // An explicit-generic-argument call resolves like an inferred one:
    // `method_call_substitution` is the single channel every instantiation
    // shape records into, explicit arguments included.
    compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        fun wrap<T: Slot>(content: T): View {
            view("p").child(content)
        }
        fun main() {
            mount("app", wrap<str>("static"));
        }
        main();
        "#,
    )
    .expect("explicit type arguments resolve the instantiation");
}

#[test]
fn an_inherited_static_default_on_a_concrete_receiver_compiles() {
    // OnType narrowing (requirement-polymorphism.md §8): the union is
    // name-keyed across ALL traits, so an unrelated needy impl under the
    // same member name spuriously fenced a concrete receiver's inherited
    // STATIC default. The receiver's head cannot change under substitution
    // — the site narrows to the members its head selects.
    compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell };
        trait Quiet {
            fun verdict(self): str {
                "quiet"
            }
        }
        impl i32 with Quiet {}
        trait Loud {
            fun verdict(self): str;
        }
        impl str with Loud {
            fun verdict(self): str {
                let s = Signal::new(1);
                s.effect(|v| {});
                "loud"
            }
        }
        fun main() {
            let d = 5.verdict();
        }
        main();
        "#,
    )
    .expect("a static inherited default is not fenced by an unrelated needy impl");
}

#[test]
fn a_needy_inherited_default_on_its_own_receiver_stays_fenced() {
    // The narrowing must not loosen the receiver's own arm: the default the
    // receiver actually inherits subscribes, and the call is uncovered.
    let errors = compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell };
        trait Loud {
            fun verdict(self): str {
                let s = Signal::new(1);
                s.effect(|v| {});
                "loud"
            }
        }
        impl i32 with Loud {}
        fun main() {
            let d = 5.verdict();
        }
        main();
        "#,
    )
    .expect_err("the receiver's own needy default keeps the fence");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn a_default_body_self_call_chain_stays_fenced() {
    // `self.inner()` inside a trait default records OnType with NO receiver
    // (the default body is shared across impls) — that arm keeps the union,
    // and a needy impl reached through it still fences.
    let errors = compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell };
        trait Chain {
            fun outer(self): str {
                self.inner()
            }
            fun inner(self): str;
        }
        impl i32 with Chain {
            fun inner(self): str {
                let s = Signal::new(1);
                s.effect(|v| {});
                "x"
            }
        }
        fun main() {
            let d = 5.outer();
        }
        main();
        "#,
    )
    .expect_err("a needy impl through a default-body self call keeps the fence");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn a_signal_through_a_closure_owned_dispatch_site_stays_fenced() {
    // 8d6980e regression (shipped in v0.21.1): an `OnConstraint` site owned
    // by a CLOSURE has no `incoming_calls` entry and no fallback arm covered
    // it, so the site contributed no coverage edges at all and the Signal
    // arm slipped the fence (requirement-polymorphism.md §1b). v0.20.0's
    // union edges fenced this shape.
    let errors = compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        import std::reactive::{ Signal, SignalCell };
        fun wrap<T: Slot>(content: T): View {
            let holder = view("p");
            let attach = || content.place(holder);
            attach();
            holder
        }
        fun main() {
            mount("app", wrap(Signal::new("live")));
        }
        main();
        "#,
    )
    .expect_err("a closure-owned dispatch site must keep the fence");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn a_static_slot_through_a_closure_owned_dispatch_site_compiles() {
    // S2 flips S1's conservative half of the §1b fix: a closure-owned site
    // resolves as its parent function does, and `wrap`'s call sites bind the
    // static arm (proven red against S1's union fallback).
    compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        fun wrap<T: Slot>(content: T): View {
            let holder = view("p");
            let attach = || content.place(holder);
            attach();
            holder
        }
        fun main() {
            mount("app", wrap("static"));
        }
        main();
        "#,
    )
    .expect("a closure-owned site resolves through its parent chain");
}

#[test]
fn a_top_level_entry_alongside_a_covered_caller_stays_fenced() {
    // Pre-existing hole (the arm is verbatim in v0.20.0): with caller edges
    // present, coverage never consulted the top-level entries, so one
    // covered caller laundered any number of uncovered top-level calls —
    // the hidden bare parameter arrived as `undefined` at runtime
    // (requirement-polymorphism.md §1c).
    let errors = compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell, run_with_owner, Owner };
        fun needy() {
            let s = Signal::new(1);
            s.effect(|v| {});
        }
        fun covered() {
            run_with_owner(Owner::new(), || needy());
        }
        fun main() {
            covered();
        }
        main();
        needy();
        "#,
    )
    .expect_err("an uncovered top-level entry fences regardless of covered callers");
    assert!(
        errors.iter().any(|error| error.contains("owner_scope")),
        "the fence must name owner_scope:\n{errors:?}"
    );
}

#[test]
fn a_covered_caller_alone_keeps_compiling() {
    // The §1c hoist must not over-tighten: the same program minus the
    // top-level entry has only covered callers and compiles.
    compile_browser(
        r#"
        import std::reactive::{ Signal, SignalCell, run_with_owner, Owner };
        fun needy() {
            let s = Signal::new(1);
            s.effect(|v| {});
        }
        fun covered() {
            run_with_owner(Owner::new(), || needy());
        }
        fun main() {
            covered();
        }
        main();
        "#,
    )
    .expect("a strict function with only covered callers compiles");
}

#[test]
fn child_of_an_unimplemented_type_names_the_slot_trait() {
    // The widened bound's failure mode: the diagnostic names the trait and
    // points at the bound, not a generic mismatch.
    assert_fails_with(
        r#"
        import std::ui::{ view, View };
        fun main() {
            let _x = view("p").child(42);
        }
        "#,
        "'i32' does not implement trait 'Slot'",
    );
}

#[test]
fn attr_of_an_unimplemented_type_names_the_attr_value_trait() {
    assert_fails_with(
        r#"
        import std::ui::{ view, View };
        fun main() {
            let _x = view("p").attr("n", true);
        }
        "#,
        "'bool' does not implement trait 'AttrValue'",
    );
}

#[test]
fn ssr_std_dom_import_fails_on_a_process_build() {
    // The boundary §2 relies on: a component reaching for raw DOM cannot SSR, and
    // the existing cross-platform gate says so at the `import` with the standard
    // error — a process build never resolves `std::dom`.
    assert_fails_with(
        r#"
        import std::dom::{ create_element };
        import std::io::print;
        fun main() {
            let element = create_element("div");
            print("built");
        }
        "#,
        "requires the `browser` layer",
    );
}

#[test]
fn ssr_on_event_is_accepted_and_discarded() {
    // `on_event` mirrors `on`: accepted and dropped. Its event type is generic
    // (the server layer cannot name the browser-only `std::dom::Event`), so a
    // handler that ignores the event renders the element and never runs.
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::io::print;
        fun main() {
            print(render(view("button").text("x").on_event("click", |_event| print("HANDLER RAN"))));
        }
        "#,
        "<button>x</button>\n",
    );
}

#[test]
fn ssr_process_build_can_import_a_browser_module_that_binds_on_event() {
    // The platform model lets a process program IMPORT a browser module as long
    // as it never reaches the browser-requiring functions (analysis stays
    // admissible). `std::router`'s `link` binds `on_event` on a `View`; the
    // process `ui` must therefore carry `on_event`, or loading `router` to color
    // the program would fail with "View has no method 'on_event'". `navigate` is
    // unreached from `main`, so the node build itself stays clean.
    assert_compiles(
        r#"
        import std::router::navigate;
        import std::io::print;
        fun unused() {
            navigate("/home");
        }
        fun main() {
            print("ok");
        }
        "#,
    );
}

// --- S2: replace semantics + the shared `app()` composition (proposal/ssr.md §1,
// §4 S2). The RUNTIME replace (mount clears before appending) needs a DOM, so it
// is pinned end-to-end under the A10 stub in `crates/vilan-cli/tests/ssr_fullstack.rs`
// (old server nodes detached, live tree in their place, bindings firing). These
// pin the compile surface and the process-leg markup the browser replaces.

#[test]
fn ssr_example_app_renders_the_served_markup() {
    // The `examples/ssr` `app()` composition — a signal-fed list, a `when`, an
    // escaped heading, and a read-once button — rendered on the process leg is the
    // exact markup the server splices into its shell: the pre-JS page the client
    // then replaces (proposal/ssr.md §1, §3).
    assert_compiles_and_runs(
        r#"
        import std::ui::{ view, View, render };
        import std::reactive::{ Signal, SignalCell };
        import std::io::print;
        fun app(): View {
            let tasks: SignalCell<List<str>> = Signal::new(["Render on the server", "Replace on boot"]);
            let show_note = Signal::new(true);
            let label = Signal::new("idle");
            view("main")
                .class("app")
                .child(view("h1").text("Tasks & <notes>"))
                .child(view("ul").bind_each(tasks, |task| task, |task| view("li").text(task)))
                .child(view("section").when(show_note, || view("p").text("server-rendered, then replaced")))
                .child(view("button").bind_text(label).on("click", || label.set("clicked")))
        }
        fun main() {
            print(render(app()));
        }
        "#,
        "<main class=\"app\"><h1>Tasks &amp; &lt;notes&gt;</h1><ul><li>Render on the server</li><li>Replace on boot</li></ul><section><p>server-rendered, then replaced</p></section><button>idle</button></main>\n",
    );
}

#[test]
fn browser_mount_surface_compiles_after_the_replace_change() {
    // The replace change (mount clears the container before appending) keeps both
    // the plain `mount` and `mount_root` compiling on the browser leg. The observable
    // clear is pinned under the DOM stub (see the module note above).
    assert_compiles_browser(
        r#"
        import std::ui::{ view, View, mount, mount_root };
        fun main() {
            mount("aside", view("div").text("live"));
            let _root = mount_root("app", || view("main").text("app"));
        }
        "#,
    );
}

// === Windows support S2: newline and BOM correctness (windows-support.md §2) ===
//
// A `\r\n` in source is ONE line terminator (spec §2), so a string literal's
// value is built from the normalized text: a multi-line literal carries `\n` per
// source line break whatever the file's on-disk encoding. The property that
// matters is byte identity — the same program checked out on Windows and on
// Linux must emit the same JavaScript — so every pin here compiles the SAME
// source twice, once as written and once as its CRLF twin, and compares the
// emitted JS byte for byte.

/// The CRLF twin of an LF source: what the same file looks like checked out
/// (or saved by an editor) with Windows line endings.
fn crlf(source: &str) -> String {
    source.replace('\n', "\r\n")
}

/// Compiles `source` and its CRLF twin and asserts the emitted JS is
/// byte-identical, returning it for further assertions.
fn assert_crlf_twin_emits_identically(source: &str) -> String {
    let lf = compile(source).expect("the LF source compiles");
    let windows = compile(&crlf(source)).expect("the CRLF twin compiles");
    assert_eq!(
        lf, windows,
        "the CRLF twin must emit byte-identical JavaScript"
    );
    lf
}

/// The one message a raw line break inside `"…"` / `i"…"` produces.
const LINE_BREAK_IN_STRING: &str = "a string cannot span lines unless it is triple-quoted";

// The single-quoted forms no longer span lines at all (the H7 disallow-revisit).
// The pins that used to prove their CRLF normalization now prove the ban, and the
// CRLF byte-identity property lives on in the triple-quoted pins below, which are
// the forms that carry multi-line text.

#[test]
fn a_multi_line_plain_string_is_rejected() {
    // What the pin used to say: a plain `"…"` spanning lines normalized its
    // `\r\n` to `\n`. It is now an error in both encodings, so the miscompile
    // class it guarded cannot arise.
    let source = "fun main(): str {\n    let text = \"alpha\nbeta\";\n    text\n}\n";
    assert_fails_with(source, LINE_BREAK_IN_STRING);
    assert_fails_with(&crlf(source), LINE_BREAK_IN_STRING);
}

#[test]
fn a_multi_line_interpolated_string_is_rejected() {
    // The form that WAS load-bearing: multi-line `i"…"` is how a macro used to
    // author the source it returns (corpus `macro-derive.vl`, migrated to
    // `i"""` with this change).
    let source = "fun main(): str {\n    let who = \"world\";\n    i\"hello {who}\nagain\"\n}\n";
    assert_fails_with(source, LINE_BREAK_IN_STRING);
    assert_fails_with(&crlf(source), LINE_BREAK_IN_STRING);
}

#[test]
fn an_unterminated_string_is_reported_on_its_own_line() {
    // The reason for the ban. Before it, the literal ran on to the NEXT `"`
    // anywhere below — here `"world"`, five lines down — and whatever the
    // compiler said, it said somewhere else entirely. The span is now the
    // opening quote of the offending literal, which is the source's FIRST `"`.
    let source = "\
fun greet(name: str): str {
    let prefix = \"hello, ;
    prefix + name
}

fun main(): str {
    greet(\"world\")
}
";
    assert_fails_spanning(source, "\"", LINE_BREAK_IN_STRING);
}

#[test]
fn code_below_a_line_break_error_still_analyzes() {
    // The salvage half (frontend.md §3): the lexer resumes AT the break, so the
    // statements under the broken literal are still lexed, parsed and CHECKED —
    // the type error below it is reported, which it could not be if the literal
    // had swallowed the rest of the file. This is what keeps the LSP useful
    // mid-edit.
    let source = "\
fun broken(): str {
    let prefix = \"hello, ;
    prefix
}

fun later(): i32 {
    let n: i32 = \"not a number\";
    n
}
";
    assert_fails_with(source, LINE_BREAK_IN_STRING);
    assert_fails_with(source, "Expected i32, but got str instead.");
}

#[test]
fn a_multi_line_triple_quoted_string_from_crlf_source_emits_lf() {
    // Triple-quoted literals already stripped CR deliberately; this pins that
    // the single-quoted forms joining them did not disturb it.
    let javascript = assert_crlf_twin_emits_identically(
        "fun main(): str {\n    \"\"\"\n    a\n    b\n    \"\"\"\n}\n",
    );
    assert!(javascript.contains(r#""a\nb""#), "{javascript}");
}

#[test]
fn a_mixed_crlf_program_emits_byte_identical_javascript() {
    // Corpus-shaped: comments, imports, several declarations, single- and
    // multi-line strings, an interpolation. The whole file, not one literal.
    let javascript = assert_crlf_twin_emits_identically(
        r#"
        import std::io::print;

        // A greeting, with a comment above it.
        fun greeting(name: str): str {
            i"hello, {name}!"
        }

        struct Note {
            title: str,
            body: str,
        }

        fun main() {
            let note = Note { title = "one", body = """
                first line
                second line
                """ };
            print(greeting(note.title));
            print(note.body);
        }
        "#,
    );
    assert!(!javascript.contains('\r'), "{javascript}");
}

#[test]
fn emitted_javascript_from_crlf_source_has_no_carriage_return_through_a_macro() {
    // The verbatim paths at once: a macro whose returned source is a multi-line
    // i-string, invoked from a CRLF file. A macro's arguments and world text are
    // raw source slices, so this is where a stray `\r` would ride into the
    // generated code and out into the bundle.
    let javascript = assert_crlf_twin_emits_identically(
        r#"
        import std::io::print;

        macro fun constants(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::meta::{ Arguments, Source };

            mut body = "";
            for name in arguments.values {
                body = body + i"""
                fun {name}(): i32 \{
                    7
                \}
                """;
            }
            source(body)
        }

        macro constants(seven);

        fun main() {
            print(seven());
        }
        "#,
    );
    assert!(!javascript.contains('\r'), "{javascript}");
    assert!(javascript.contains("function seven()"), "{javascript}");
}

#[test]
fn a_macro_observing_a_multi_line_argument_sees_lf_from_crlf_source() {
    // The macro layer hands a macro its argument TEXT as a VALUE (`Arguments`,
    // `Field`, `FunctionItem`), so §2's rule applies there too (S3's tail): a
    // macro that MEASURES or string-compares a multi-line argument must see the
    // same text whatever the file's on-disk encoding. The argument below is
    // deliberately laid out so its text is exactly `1 +\n2` (5 bytes) — an
    // un-normalized CRLF twin measures 6 and `width()` returns a different
    // number, which the byte-identity assertion catches.
    let javascript = assert_crlf_twin_emits_identically(
        r#"
        import std::io::print;

        macro fun measure(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::meta::{ Arguments, Source };

            let text = arguments.values[0];
            source(i"""
            fun width(): i32 \{
                {text.len()}
            \}
            """)
        }

        macro measure(1 +
2);

        fun main() {
            print(width());
        }
        "#,
    );
    assert!(javascript.contains("return 5"), "{javascript}");
    assert!(!javascript.contains('\r'), "{javascript}");
}

#[test]
fn a_lone_carriage_return_ends_a_string_literal() {
    // Classic-Mac endings are still NOT blessed as line terminators
    // (windows-support.md §2 — `normalize_newlines` leaves a lone `\r` alone),
    // but a lone `\r` DOES end a single-quoted literal: whatever the file's
    // convention, the closing quote is not on this line. The pin that used to
    // assert `"a\rb"` compiles to a value with a CR now asserts the ban.
    assert_fails_with("fun main(): str {\n    \"a\rb\"\n}\n", LINE_BREAK_IN_STRING);
}

#[test]
fn a_backslash_before_a_line_break_in_an_interpolated_string_is_rejected() {
    // The i-string fragment scanner ends an escape on a character COUNT, so a
    // `\` immediately before a line break used to end its fragment BETWEEN the
    // CR and the LF — one line terminator split across two `String` tokens,
    // where per-token normalization can no longer see the pair, and the CR rode
    // into the value. The ban removes the shape: nothing escapes a line break,
    // so the split can no longer happen in a single-quoted literal at all.
    for source in [
        "fun main(): str {\n    i\"a\\\nb\"\n}\n",
        "fun main(): str {\n    i\"a\\\r\nb\"\n}\n",
    ] {
        assert_fails_with(source, LINE_BREAK_IN_STRING);
    }
}

#[test]
fn an_interpolated_triple_quoted_string_from_crlf_source_emits_lf() {
    // H7's literal fragments per LINE, so every line terminator sits at a
    // fragment boundary — the shape most exposed to a split CRLF pair.
    let javascript = assert_crlf_twin_emits_identically(
        "fun main(): str {\n    let who = \"w\";\n    i\"\"\"\n    a {who}\n    b\n    \"\"\"\n}\n",
    );
    assert!(!javascript.contains(r"\r"), "{javascript}");
}

#[test]
fn a_backslash_before_a_crlf_line_break_in_an_interpolated_triple_quoted_string_emits_lf() {
    // The H7 twin of the case above, and the one the trimming complicates: a
    // trailing `\` on the LAST content line has no terminator to take (the
    // trimming removed it), while one on an interior line takes the whole pair.
    let javascript = assert_crlf_twin_emits_identically(
        "fun main(): str {\n    i\"\"\"\n    a\\\n    b\\\n    \"\"\"\n}\n",
    );
    assert!(!javascript.contains(r"\r"), "{javascript}");
    assert!(javascript.contains(r#""a\\\nb\\""#), "{javascript}");
}

#[test]
fn a_backslash_before_a_line_break_in_a_plain_string_is_rejected() {
    // The plain `"…"` twin of the case above. Its body was ONE contiguous token,
    // so the CRLF pair could never split there — but a `\` before a line break
    // is the same ban in both forms, so the rule needs no per-form exception.
    for source in [
        "fun main(): str {\n    \"a\\\nb\"\n}\n",
        "fun main(): str {\n    \"a\\\r\nb\"\n}\n",
    ] {
        assert_fails_with(source, LINE_BREAK_IN_STRING);
    }
}

#[test]
fn an_escape_immediately_before_a_line_break_is_still_the_ban() {
    // The multi-escape edge: a real escape adjacent to the line break, so the
    // fragment boundary lands right at the CR from the other side. `\\` then a
    // break, and `\n` then a break — the break rules in both cases.
    for source in [
        "fun main(): str {\n    i\"a\\\\\nb\"\n}\n",
        "fun main(): str {\n    i\"a\\n\nb\"\n}\n",
    ] {
        assert_fails_with(source, LINE_BREAK_IN_STRING);
    }
}

#[test]
fn a_line_break_after_a_hole_is_the_ban() {
    // …and with a hole before it, so the break is not in the i-string's first
    // fragment. The salvage keeps the hole's tokens, so nothing downstream
    // panics on a half-scanned concatenation.
    assert_fails_with(
        "fun main(): str {\n    let n = \"x\";\n    i\"a{n}\\\nb\"\n}\n",
        LINE_BREAK_IN_STRING,
    );
}

#[test]
fn a_backslash_before_a_crlf_break_after_a_hole_in_a_triple_quoted_string_emits_lf() {
    // The surviving CRLF-pair case: `lex_multiline_escape` is now the ONLY
    // count-based fragment scanner that can meet a line terminator, so this is
    // the pin that keeps its pair handling honest. A hole precedes the escape,
    // so the fragment it starts is not the literal's first.
    let javascript = assert_crlf_twin_emits_identically(
        "fun main(): str {\n    let n = \"x\";\n    i\"\"\"\n    a{n}\\\n    b\n    \"\"\"\n}\n",
    );
    assert!(!javascript.contains(r"\r"), "{javascript}");
}

// --- Path semantics: library territory is decided on canonicalized paths ---
// (windows-support.md §5)

/// Analyzes `entry` (whose text is `source`) against the real std spec for the
/// browser platform, and returns how many of the ENTRY file's own functions
/// platform coloring gave a layer requirement — the observable that says
/// whether the file was recognized as library territory or silently demoted to
/// "user code".
fn entry_functions_with_a_requirement(entry: &Path) -> (usize, usize) {
    let std = std_spec();
    let source: &'static str = Box::leak(
        std::fs::read_to_string(entry)
            .expect("read the entry module")
            .into_boxed_str(),
    );
    let (program, _diagnostics) = analyze_source(
        source,
        &std,
        &std.base_root,
        entry,
        Some(Platform::Browser),
        &Workspace::default(),
    );
    let program = program.expect("the module analyzes");
    let requirements = vilan_core::platform_color::requirements(&program);
    let entry_functions: Vec<_> = program
        .functions
        .keys()
        .filter(|id| program.source_of(**id) == Some(vilan_core::analyzer::SourceId(0)))
        .collect();
    let described = entry_functions
        .iter()
        .filter(|id| requirements.contains_key(**id))
        .count();
    (described, entry_functions.len())
}

// A symlink is the portable-on-unix way to give one file two spellings that
// only `canonicalize` can reconcile. On Windows the same disagreement is
// unconditional (a canonicalized root carries the `\\?\` verbatim prefix, a
// join-built path never does), and the windows-latest CI leg is that half.
#[cfg(unix)]
#[test]
fn a_library_module_reached_through_a_symlink_is_still_library_territory() {
    // Platform coloring tests each source path against the library LAYER ROOTS.
    // The two sides are produced by different routes — a root from the package
    // spec, a source from whatever path the caller opened — so the comparison
    // is only sound once BOTH go through `util::canonical_path`. Reached
    // through this symlink the raw paths share no prefix at all, so without the
    // canonicalization the module's functions lose their layer requirement
    // entirely: a library frame silently demoted to user code, which is a wrong
    // platform diagnostic rather than a missing one.
    let browser = std_spec()
        .layers
        .iter()
        .find(|layer| layer.name == "browser")
        .expect("std has a browser layer")
        .root
        .clone();
    let real = browser
        .canonicalize()
        .expect("the browser layer is on disk");

    let scratch = std::env::temp_dir().join(format!(
        "vilan-layer-symlink-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("create the scratch directory");
    let link = scratch.join("layer");
    std::os::unix::fs::symlink(&real, &link).expect("symlink the browser layer");

    let through_link = link.join("dev.vl");
    assert!(
        !through_link.starts_with(&browser) && !through_link.starts_with(&real),
        "the pin needs a spelling that shares no prefix with the recorded root"
    );

    let (described, total) = entry_functions_with_a_requirement(&through_link);
    assert!(total > 0, "the opened module defines functions");
    assert_eq!(
        described, total,
        "every function of a browser-layer module keeps its layer's requirement \
         when the module is reached by a different spelling of its root"
    );

    // The control: the same file by its ordinary spelling behaves identically,
    // so the assertion above is about the SPELLING and not about `dev.vl`.
    let (direct_described, direct_total) = entry_functions_with_a_requirement(&real.join("dev.vl"));
    assert_eq!((direct_described, direct_total), (described, total));

    let _ = std::fs::remove_dir_all(&scratch);
}

// --- B33: module initialization order (b33-emission-order.md) --------------

/// Compile a MULTI-FILE package: `files` (relative path → contents) are written
/// into a fresh temp directory used as the package root, `entry` is analyzed
/// against it, and the emitted JS comes back. The B33 pins need real modules on
/// disk — the load-time relation and the canonical tie-break both span files,
/// and the naive-sort counterexample is only expressible across two of them.
fn compile_package(files: &[(&str, &str)], entry: &str) -> Result<String, Vec<String>> {
    let outcome = analyze_package(files, entry);
    match outcome.javascript {
        Some(javascript) => Ok(javascript),
        None => Err(outcome
            .diagnostics
            .into_iter()
            .map(|(message, _span, _file)| message)
            .collect()),
    }
}

/// What compiling a multi-file package produced: the JS if it compiled, and
/// every diagnostic with its span AND the file it is attributed to
/// (`Program::diagnostic_sources` — what the editor publishes it against).
/// A cross-module diagnostic can only be pinned to a *file* through this.
struct PackageOutcome {
    javascript: Option<String>,
    diagnostics: Vec<(String, std::ops::Range<usize>, Option<String>)>,
}

fn analyze_package(files: &[(&str, &str)], entry: &str) -> PackageOutcome {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("vilan_init_order_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    for (relative, contents) in files {
        let path = directory.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
    }
    let entry_path = directory.join(entry);
    let source = std::fs::read_to_string(&entry_path).unwrap();

    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let leaked: &'static str = Box::leak(source.into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    &directory,
                    &entry_path,
                    Some(Platform::default()),
                    &Workspace::default(),
                );
                // `errors` is the entry's own parse errors followed by the
                // program's, and `diagnostic_sources` is parallel to the
                // program's half — the same arithmetic the language server does.
                let prefix = errors.len()
                    - program
                        .as_ref()
                        .map(|program| program.diagnostics.len())
                        .unwrap_or(0);
                let mut diagnostics: Vec<(String, std::ops::Range<usize>, Option<String>)> = errors
                    .iter()
                    .enumerate()
                    .map(|(index, error)| {
                        let file = index.checked_sub(prefix).and_then(|offset| {
                            let program = program.as_ref()?;
                            let source = program.diagnostic_sources.get(offset)?;
                            let path = program.source_path(*source)?;
                            Some(path.file_name()?.to_string_lossy().into_owned())
                        });
                        (error.msg.clone(), error.span.into_range(), file)
                    })
                    .collect();
                let javascript = match program {
                    Some(program) if errors.is_empty() => {
                        match transform(&program, &BuildOptions::default()) {
                            Ok(javascript) => Some(javascript),
                            Err(error) => {
                                diagnostics.push((error.msg, error.span.into_range(), None));
                                None
                            }
                        }
                    }
                    _ => None,
                };
                let _ = std::fs::remove_dir_all(&directory);
                PackageOutcome {
                    javascript,
                    diagnostics,
                }
            }))
            .unwrap_or_else(|_| PackageOutcome {
                javascript: None,
                diagnostics: vec![("compiler panicked".to_string(), 0..0, None)],
            })
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| PackageOutcome {
            javascript: None,
            diagnostics: vec![("compiler thread aborted".to_string(), 0..0, None)],
        })
}

/// [`compile_package`] plus a `node` run: returns `(emitted JS, stdout)`.
fn compile_and_run_package(
    files: &[(&str, &str)],
    entry: &str,
) -> Result<(String, String), Vec<String>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let js = compile_package(files, entry)?;
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_init_order_run_{}_{unique}.js",
        std::process::id()
    ));
    std::fs::write(&path, &js).map_err(|error| vec![error.to_string()])?;
    let output = std::process::Command::new("node").arg(&path).output();
    let _ = std::fs::remove_file(&path);
    match output {
        Ok(output) if output.status.success() => {
            Ok((js, String::from_utf8_lossy(&output.stdout).into_owned()))
        }
        Ok(output) => Err(vec![String::from_utf8_lossy(&output.stderr).into_owned()]),
        Err(error) => Err(vec![format!("could not run node: {error}")]),
    }
}

/// The index at which `needle` appears in `js`, for asserting relative
/// declaration order.
#[track_caller]
fn declaration_position(js: &str, needle: &str) -> usize {
    js.find(needle)
        .unwrap_or_else(|| panic!("emitted JS has no `{needle}`:\n{js}"))
}

#[test]
fn module_binding_may_reference_one_declared_below_it() {
    // B33 pin 1 (§1's first consequence): same-module bindings are order-free.
    // Before the dependency order this built cleanly and crashed at load with
    // `Cannot access 'B' before initialization` — `const` is not hoisted, and
    // emission followed declaration order.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        let A: i32 = B * 2;
        let B: i32 = 21;
        fun main() {
            print(A);
            print(B);
        }
        "#,
        "42\n21\n",
    );
}

#[test]
fn a_dependency_in_a_later_loading_module_is_declared_first() {
    // B33 pin 2 — the naive-sort counterexample from the proposal's §0, stated
    // as a program: `alpha` loads BEFORE `zeta` canonically (module names sort),
    // so `A`'s entity id is lower than `Z`'s — yet `A`'s initializer reads `Z`.
    // Sorting by the canonical key alone (id or name) emits `A` first and
    // TDZ-crashes; the load-time relation puts `Z` first.
    let (js, stdout) = compile_and_run_package(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::alpha::{ A };\nimport pkg::zeta::{ Z };\n\
                 fun main() { print(A); print(Z); }\n",
            ),
            (
                "alpha.vl",
                "import pkg::zeta::{ Z };\nlet A: i32 = Z * 2;\n",
            ),
            ("zeta.vl", "let Z: i32 = 21;\n"),
        ],
        "main.vl",
    )
    .expect("expected a clean run");
    assert_eq!(stdout, "42\n21\n");
    assert!(
        declaration_position(&js, "const Z = 21;") < declaration_position(&js, "const A = Z * 2;"),
        "the dependency must be DECLARED first, not merely happen to run:\n{js}"
    );
}

#[test]
fn import_statement_order_cannot_change_module_binding_order() {
    // The other half of pin 2: the SAME program with its two imports swapped
    // emits identical bytes. Before B33 this flipped the declaration order (the
    // entry scope's insertion order decided it) and one spelling TDZ-crashed.
    let module_files: [(&str, &str); 2] = [
        (
            "alpha.vl",
            "import pkg::zeta::{ Z };\nlet A: i32 = Z * 2;\n",
        ),
        ("zeta.vl", "let Z: i32 = 21;\n"),
    ];
    let alpha_first = "import std::io::print;\nimport pkg::alpha::{ A };\nimport pkg::zeta::{ Z };\n\
                       fun main() { print(A); print(Z); }\n";
    let zeta_first = "import std::io::print;\nimport pkg::zeta::{ Z };\nimport pkg::alpha::{ A };\n\
                      fun main() { print(A); print(Z); }\n";

    let mut with_alpha_first = vec![("main.vl", alpha_first)];
    with_alpha_first.extend(module_files);
    let mut with_zeta_first = vec![("main.vl", zeta_first)];
    with_zeta_first.extend(module_files);

    let first = compile_package(&with_alpha_first, "main.vl").expect("clean compile");
    let second = compile_package(&with_zeta_first, "main.vl").expect("clean compile");
    assert_eq!(
        first, second,
        "permuting the import statements must not change a byte"
    );
}

#[test]
fn mutually_recursive_module_closures_stay_legal() {
    // B33 pin 3 — the §5(a) guard. EVEN and ODD each CREATE a closure whose body
    // calls the other; neither EVALUATES the other at load. Creation is inert, so
    // the relation has no edge here and no cycle. Building the order on the call
    // graph's raw `successors` would charge each body to its creator and reject
    // this working program.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        let EVEN: |i32| bool = |n: i32| {
            if n == 0 { true } else { ODD(n - 1) }
        };
        let ODD: |i32| bool = |n: i32| {
            if n == 0 { false } else { EVEN(n - 1) }
        };
        fun main() {
            print(EVEN(4));
            print(ODD(4));
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn edgeless_module_closures_emit_in_canonical_order() {
    // The second half of pin 3: with NO edges between them, EVEN and ODD fall to
    // the canonical tie-break — declaration order within the file, which is
    // entity-id order. (This is also what proves the relation found no edge: a
    // spurious one would have to reorder them or make them cycle leftovers.)
    let js = compile(
        r#"
        import std::io::print;
        let EVEN: |i32| bool = |n: i32| {
            if n == 0 { true } else { ODD(n - 1) }
        };
        let ODD: |i32| bool = |n: i32| {
            if n == 0 { false } else { EVEN(n - 1) }
        };
        fun main() { print(EVEN(4)); print(ODD(4)); }
        "#,
    )
    .expect("clean compile");
    assert!(
        declaration_position(&js, "const EVEN =") < declaration_position(&js, "const ODD ="),
        "no edges means canonical order:\n{js}"
    );
}

#[test]
fn a_call_through_a_global_orders_what_that_body_reads() {
    // B33 pin 4 — §2's "call through a value". `X`'s initializer calls `FETCH`,
    // a binding holding a closure; the closure's body reads `Y`, so `Y` charges
    // to X (the EVALUATOR), not to FETCH. Probed before the fix: `Y` emitted
    // last and the run died with `Cannot access 'Y' before initialization`.
    let js = compile(
        r#"
        import std::io::print;
        let FETCH: || i32 = || { Y };
        let X: i32 = FETCH();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
    )
    .expect("clean compile");
    assert!(
        declaration_position(&js, "const Y = 7;") < declaration_position(&js, "const X ="),
        "the closure body's read must order Y before X:\n{js}"
    );
    assert!(
        declaration_position(&js, "const FETCH =") < declaration_position(&js, "const Y = 7;"),
        "FETCH itself stays UNORDERED w.r.t. Y — canonical order keeps it first:\n{js}"
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        let FETCH: || i32 = || { Y };
        let X: i32 = FETCH();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "7\n",
    );
}

#[test]
fn a_direct_call_at_init_orders_what_the_callee_reads() {
    // The transitive half of §2: a plain function call at init is entered, and
    // the callee's global reads charge to the initializing binding.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun read_y(): i32 { Y * 3 }
        let X: i32 = read_y();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "21\n",
    );
}

#[test]
fn unrelated_effectful_initializers_run_in_canonical_order() {
    // B33 pin 5 — the §5(d) spec pin. Two initializers with NO dependency
    // between them, in two modules: their observable order is the canonical one
    // (module name, so `alpha` before `zeta`) whatever order the ENTRY lists its
    // imports in. Before B33 the entry's import listing decided, so this printed
    // "zeta" first.
    let (_js, stdout) = compile_and_run_package(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::zeta::{ Z };\nimport pkg::alpha::{ A };\n\
                 fun main() { print(A + Z); }\n",
            ),
            (
                "util.vl",
                "import std::io::print;\nfun announce(label: str): i32 { print(label); 1 }\n",
            ),
            (
                "alpha.vl",
                "import pkg::util::{ announce };\nlet A: i32 = announce(\"alpha\");\n",
            ),
            (
                "zeta.vl",
                "import pkg::util::{ announce };\nlet Z: i32 = announce(\"zeta\");\n",
            ),
        ],
        "main.vl",
    )
    .expect("expected a clean run");
    assert_eq!(stdout, "alpha\nzeta\n2\n");
}

#[test]
fn a_const_binding_still_folds_and_orders_as_a_plain_value() {
    // B33 pin 6. A `const`-marked initializer runs in the compile-time
    // interpreter, so the call graph never collects it and it has NO outgoing
    // edges. It stays a legitimate ordering TARGET, though: the folded
    // `const STEP = 7;` declaration must still precede the binding that reads it.
    let js = compile(
        r#"
        import std::io::print;
        fun seven(): i32 { 7 }
        let DOUBLE: i32 = STEP * 2;
        let STEP: i32 = const seven();
        fun main() { print(DOUBLE); }
        "#,
    )
    .expect("clean compile");
    assert!(
        js.contains("const STEP = 7;"),
        "the const initializer still folds to a literal:\n{js}"
    );
    assert!(
        declaration_position(&js, "const STEP = 7;") < declaration_position(&js, "const DOUBLE ="),
        "a const binding is still ordered before its reader:\n{js}"
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun seven(): i32 { 7 }
        let DOUBLE: i32 = STEP * 2;
        let STEP: i32 = const seven();
        fun main() { print(DOUBLE); }
        "#,
        "14\n",
    );
}

#[test]
fn a_dispatching_initializer_is_accepted_and_ordered() {
    // B33 pin 7 — the §5(b) risk probe. `TOTAL`'s initializer calls a
    // trait-bounded generic, so the relation follows EVERY dispatch candidate
    // (the standing over-approximation): both `weight` impls read `BASE`, and
    // `total` itself reads `OFFSET`. No real cycle exists, so the program is
    // accepted and both reads are ordered before `TOTAL`.
    let js = compile(
        r#"
        import std::io::print;
        trait Weight { fun weight(self): i32; }
        struct Feather {}
        struct Anvil {}
        impl Feather with Weight { fun weight(self): i32 { BASE } }
        impl Anvil with Weight { fun weight(self): i32 { BASE * 100 } }
        fun total<T: Weight>(item: T): i32 { item.weight() + OFFSET }
        let TOTAL: i32 = total(Feather {});
        let BASE: i32 = 3;
        let OFFSET: i32 = 1;
        fun main() { print(TOTAL); print(total(Anvil {})); }
        "#,
    )
    .expect("clean compile");
    let total = declaration_position(&js, "const TOTAL =");
    assert!(
        declaration_position(&js, "const BASE = 3;") < total
            && declaration_position(&js, "const OFFSET = 1;") < total,
        "both candidates' reads order before the dispatching initializer:\n{js}"
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Weight { fun weight(self): i32; }
        struct Feather {}
        struct Anvil {}
        impl Feather with Weight { fun weight(self): i32 { BASE } }
        impl Anvil with Weight { fun weight(self): i32 { BASE * 100 } }
        fun total<T: Weight>(item: T): i32 { item.weight() + OFFSET }
        let TOTAL: i32 = total(Feather {});
        let BASE: i32 = 3;
        let OFFSET: i32 = 1;
        fun main() { print(TOTAL); print(total(Anvil {})); }
        "#,
        "4\n301\n",
    );
}

#[test]
fn a_self_referential_binding_is_an_initialization_cycle() {
    // B33 S2 pin 1 — the degenerate cycle. `let A = A + 1` emitted
    // `const A = A + 1;` and TDZ-crashed at load; S1 pinned that status quo
    // (`a_self_referential_binding_still_builds_in_s1`) precisely so this flip
    // would be deliberate. It is now an error, worded for what it is — a
    // binding evaluating itself — rather than as a `via A → A` chain.
    assert_fails_with(
        r#"
        import std::io::print;
        let A: i32 = A + 1;
        fun main() { print(A); }
        "#,
        "`A`'s initializer evaluates `A` itself, which has not initialized yet",
    );
}

#[test]
fn a_self_referential_binding_is_spanned_at_the_read_and_carries_no_note() {
    // The anchor rule (diagnostics-standard A1): the primary span is the READ
    // that closes the cycle, not the whole `let`. And the C3 note is dropped
    // when it would add nothing — here the declaration CONTAINS the anchored
    // read, so "`A` is declared here" would point at what the reader is
    // already looking at.
    let source = r#"
        import std::io::print;
        let A: i32 = A + 1;
        fun main() { print(A); }
        "#;
    assert_fails_spanning_nth(source, "A", 1, "evaluates `A` itself");
    let diagnostics = failure_diagnostics_with_notes(source);
    assert_eq!(diagnostics.len(), 1, "one diagnostic: {diagnostics:#?}");
    assert!(
        diagnostics[0].2.is_none(),
        "a self-cycle's declaration note is redundant and dropped: {diagnostics:#?}"
    );
}

#[test]
fn a_cycle_does_not_disturb_the_bindings_around_it() {
    // A cycle must not scramble the rest of the program — the property S1's
    // condensation bought. Under S2 the program no longer compiles, so the
    // ORDER is pinned where it can be observed directly: over the synthetic
    // relation, in `init_order.rs`'s unit tests (`a_self_dependency_is_its_own
    // _component`, `a_cycle_does_not_displace_unrelated_bindings`). What is
    // still observable here is that the unrelated binding is not dragged into
    // the diagnostic: exactly one error, naming only the cycle's member.
    let diagnostics = failure_diagnostics(
        r#"
        import std::io::print;
        let A: i32 = A + 1;
        let OK: i32 = 5;
        fun main() { print(OK); print(A); }
        "#,
    );
    assert_eq!(diagnostics.len(), 1, "one diagnostic: {diagnostics:#?}");
    assert!(
        !diagnostics[0].0.contains("OK"),
        "a binding outside the cycle is never named: {diagnostics:#?}"
    );
}

#[test]
fn a_call_through_a_closure_built_by_a_function_is_ordered() {
    // §2's "def chain": `MAKER`'s value came out of `make()`, so the call
    // `MAKER()` can reach any closure `make` creates — and that closure's read
    // of `Y` charges to `X`. Note `MAKER` itself stays unordered w.r.t. `Y`
    // (`make`'s own body reads nothing).
    let js = compile(
        r#"
        import std::io::print;
        fun make(): || i32 { || { Y } }
        let MAKER: || i32 = make();
        let X: i32 = MAKER();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
    )
    .expect("clean compile");
    assert!(
        declaration_position(&js, "const Y = 7;") < declaration_position(&js, "const X ="),
        "the created closure's read must order Y before X:\n{js}"
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun make(): || i32 { || { Y } }
        let MAKER: || i32 = make();
        let X: i32 = MAKER();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "7\n",
    );
}

#[test]
fn a_call_through_a_struct_field_closure_is_ordered() {
    // A closure reached by PROJECTION out of a binding: the field read resolves
    // to the binding, whose initializer created the closure. Probed as a live
    // TDZ before the projection arms existed.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        struct Holder { get: || i32 }
        let HOLDER: Holder = Holder { get = || { Y } };
        let X: i32 = (HOLDER.get)();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "7\n",
    );
}

#[test]
fn a_call_through_an_indexed_closure_is_ordered() {
    // The same projection rule through a list index and a tuple index — three
    // distinct `Expr` arms, so three cases.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        let TABLE: List<|| i32> = [|| { Y }];
        let X: i32 = TABLE[0]();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "7\n",
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        let PAIR: (|| i32, i32) = (|| { Y }, 1);
        let X: i32 = (PAIR.0)();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "7\n",
    );
}

#[test]
fn a_const_binding_contributes_no_ordering_edges() {
    // The other half of pin 6: a `const`-marked initializer is EXEMPT as a
    // source. `STEP` reads `BASE`, but both fold at compile time, so neither the
    // emitted code nor the relation carries that read — `STEP` keeps its
    // canonical (declaration-first) position instead of being pushed after
    // `BASE`, and no cycle can be manufactured out of const chains.
    let js = compile(
        r#"
        import std::io::print;
        let STEP: i32 = const BASE * 2;
        let BASE: i32 = const 6;
        fun main() { print(STEP); print(BASE); }
        "#,
    )
    .expect("clean compile");
    assert!(
        js.contains("const STEP = 12;") && js.contains("const BASE = 6;"),
        "both fold to literals:\n{js}"
    );
    assert!(
        declaration_position(&js, "const STEP = 12;")
            < declaration_position(&js, "const BASE = 6;"),
        "a folded read is not an ordering edge — canonical order stands:\n{js}"
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        let STEP: i32 = const BASE * 2;
        let BASE: i32 = const 6;
        fun main() { print(STEP); print(BASE); }
        "#,
        "12\n6\n",
    );
}

// --- B33 S1, adversarial-review round: values handed to a load-time call ----
//
// A function VALUE passed into a call that runs at load may be invoked by the
// callee. Before the review, the relation resolved only a call's SUBJECT, so
// every shape below lost the closure body's global read — and since the
// surrounding order is now DERIVED, a lost edge is a live miscompile, not a
// preserved status quo. Each is cross-module with the dependency in the
// LATER-loading module (`zeta` > `alpha`), which is what makes the canonical
// tie-break put the reader first unless the edge exists. Each was probed
// TDZ-crashing before the fix.

/// The shared entry for the argument-passing fixtures: `alpha` holds the
/// binding under test, `zeta` holds the global its closure reads.
const ARGUMENT_ENTRY: &str = "import std::io::print;\nimport pkg::zeta::{ Y };\n\
                              import pkg::alpha::{ A };\nfun main() { print(A); }\n";

#[test]
fn a_closure_global_passed_as_an_argument_is_entered() {
    // (a) `apply(CB)` — CB is a module binding holding a closure; `apply` calls
    // it, so CB's body's read of `Y` charges to `A`.
    let (_js, stdout) = compile_and_run_package(
        &[
            ("main.vl", ARGUMENT_ENTRY),
            ("zeta.vl", "let Y: i32 = 7;\n"),
            (
                "alpha.vl",
                "import pkg::zeta::{ Y };\nlet CB: || i32 = || { Y };\n\
                 fun apply(f: || i32): i32 { f() }\nlet A: i32 = apply(CB);\n",
            ),
        ],
        "main.vl",
    )
    .expect("expected a clean run");
    assert_eq!(stdout, "7\n");
}

#[test]
fn an_inline_closure_argument_is_entered() {
    // (b) `apply(|| { Y })` — the closure never becomes a binding at all, so
    // `A`'s initializer had NO edges before the fix.
    let (_js, stdout) = compile_and_run_package(
        &[
            ("main.vl", ARGUMENT_ENTRY),
            ("zeta.vl", "let Y: i32 = 7;\n"),
            (
                "alpha.vl",
                "import pkg::zeta::{ Y };\nfun apply(f: || i32): i32 { f() }\n\
                 let A: i32 = apply(|| { Y });\n",
            ),
        ],
        "main.vl",
    )
    .expect("expected a clean run");
    assert_eq!(stdout, "7\n");
}

#[test]
fn a_closure_argument_to_a_std_iterator_method_is_entered() {
    // (c) The plain-idiom case: `LIST.map(|e| e + Y)`. `map` lowers through an
    // emitted helper, so following only resolved CALL TARGETS dead-ends and the
    // closure's read of `Y` vanished. Nothing about this program is exotic.
    let (_js, stdout) = compile_and_run_package(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::zeta::{ Y };\nimport pkg::alpha::{ A };\n\
                 fun main() { print(A.len()); }\n",
            ),
            ("zeta.vl", "let Y: i32 = 7;\n"),
            (
                "alpha.vl",
                "import pkg::zeta::{ Y };\nlet LIST: List<i32> = [1, 2, 3];\n\
                 let A: List<i32> = LIST.map(|e: i32| { e + Y });\n",
            ),
        ],
        "main.vl",
    )
    .expect("expected a clean run");
    assert_eq!(stdout, "3\n");
}

#[test]
fn a_method_receivers_field_closure_is_entered() {
    // (d) `HOLDER.run()`, where `run` invokes `(self.get)()`. The receiver is
    // argument 0, so resolving a call's arguments reaches the closures `HOLDER`
    // holds; resolving only the subject (the method) reached nothing.
    let (_js, stdout) = compile_and_run_package(
        &[
            ("main.vl", ARGUMENT_ENTRY),
            ("zeta.vl", "let Y: i32 = 7;\n"),
            (
                "alpha.vl",
                "import pkg::zeta::{ Y };\nstruct Holder { get: || i32 }\n\
                 impl Holder { fun run(self): i32 { (self.get)() } }\n\
                 let HOLDER: Holder = Holder { get = || { Y } };\nlet A: i32 = HOLDER.run();\n",
            ),
        ],
        "main.vl",
    )
    .expect("expected a clean run");
    assert_eq!(stdout, "7\n");
}

#[test]
fn a_two_level_def_chain_is_followed() {
    // The def chain must reach through the callee's OWN calls: `make` creates
    // nothing itself — `inner` does — so reading only the immediate callee's
    // created closures missed `Y`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun inner(): || i32 { || { Y } }
        fun make(): || i32 { inner() }
        let MAKER: || i32 = make();
        let X: i32 = MAKER();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "7\n",
    );
}

#[test]
fn a_conditional_call_subject_enters_both_branches() {
    // `(if FLAG { CB_A } else { CB_B })()` — a reachable call subject whose
    // value is an `Expr::If`. Both branch values must be entered; the
    // exhaustive match is what forces this arm to exist.
    let js = compile(
        r#"
        import std::io::print;
        let FLAG: bool = true;
        let CB_A: || i32 = || { Y };
        let CB_B: || i32 = || { 0 };
        let X: i32 = (if FLAG { CB_A } else { CB_B })();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
    )
    .expect("clean compile");
    assert!(
        declaration_position(&js, "const Y = 7;") < declaration_position(&js, "const X ="),
        "either branch's body can run, so its reads order before X:\n{js}"
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        let FLAG: bool = true;
        let CB_A: || i32 = || { Y };
        let CB_B: || i32 = || { 0 };
        let X: i32 = (if FLAG { CB_A } else { CB_B })();
        let Y: i32 = 7;
        fun main() { print(X); }
        "#,
        "7\n",
    );
}

#[test]
fn a_dispatch_manufactured_cycle_is_an_error_that_explains_the_over_approximation() {
    // B33 S2 pin 5 — the §5(b) call, ratified (b): ship STRICT. The
    // over-approximation can manufacture a cycle out of an implementation this
    // program never instantiates — `TOTAL` calls a trait-bounded generic with a
    // `Feather`, and it is `Anvil`'s `weight` that reads `TOTAL` — and that is
    // an error all the same, with the full chain, so a false positive is
    // self-explaining. S1 pinned this fixture as a clean run
    // (`a_binding_downstream_of_a_false_cycle_still_orders_after_it`, which
    // proved the condensation kept `DOWNSTREAM` ordered after the false cycle);
    // that ORDERING property now lives in `init_order.rs`'s unit tests over the
    // synthetic relation, where a rejected program cannot hide it.
    let errors = compile_package(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::zeta::{ TOTAL, total, Anvil };\n\
                 import pkg::alpha::{ DOWNSTREAM };\n\
                 fun main() { print(DOWNSTREAM); print(total(Anvil {})); }\n",
            ),
            (
                "zeta.vl",
                "trait Weight { fun weight(self): i32; }\nstruct Feather {}\nstruct Anvil {}\n\
                 impl Feather with Weight { fun weight(self): i32 { 1 } }\n\
                 impl Anvil with Weight { fun weight(self): i32 { TOTAL } }\n\
                 fun total<T: Weight>(item: T): i32 { item.weight() }\n\
                 let TOTAL: i32 = total(Feather {});\n",
            ),
            (
                "alpha.vl",
                "import pkg::zeta::{ TOTAL };\nlet DOWNSTREAM: i32 = TOTAL + 1;\n",
            ),
        ],
        "main.vl",
    )
    .expect_err("a dispatch-manufactured cycle is rejected under the ratified (b) call");
    assert_eq!(errors.len(), 1, "one diagnostic per cycle: {errors:#?}");
    assert!(
        errors[0].contains("`TOTAL`'s initializer evaluates `TOTAL` itself"),
        "the cycle is reported: {errors:#?}"
    );
    assert!(
        errors[0].contains(
            "the cycle runs through a dispatched call, so it includes every implementation \
             of that method; one this program never instantiates still participates"
        ),
        "the over-approximation states itself in the diagnostic: {errors:#?}"
    );
}

#[test]
fn a_binding_downstream_of_a_cycle_is_not_named_in_the_error() {
    // B33 S2 pin 6. `DOWNSTREAM` reads a cycle member; it is not a member
    // itself, so it is not a participant and is never named — only true members
    // are. (Same fixture as the pin above: the point here is what the message
    // does NOT say.)
    let errors = compile_package(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::zeta::{ TOTAL, total, Anvil };\n\
                 import pkg::alpha::{ DOWNSTREAM };\n\
                 fun main() { print(DOWNSTREAM); print(total(Anvil {})); }\n",
            ),
            (
                "zeta.vl",
                "trait Weight { fun weight(self): i32; }\nstruct Feather {}\nstruct Anvil {}\n\
                 impl Feather with Weight { fun weight(self): i32 { 1 } }\n\
                 impl Anvil with Weight { fun weight(self): i32 { TOTAL } }\n\
                 fun total<T: Weight>(item: T): i32 { item.weight() }\n\
                 let TOTAL: i32 = total(Feather {});\n",
            ),
            (
                "alpha.vl",
                "import pkg::zeta::{ TOTAL };\nlet DOWNSTREAM: i32 = TOTAL + 1;\n",
            ),
        ],
        "main.vl",
    )
    .expect_err("the cycle is rejected");
    assert_eq!(errors.len(), 1, "one diagnostic per cycle: {errors:#?}");
    assert!(
        !errors[0].contains("DOWNSTREAM"),
        "a binding merely downstream of the cycle is not a participant: {errors:#?}"
    );
}

#[test]
fn an_unreachable_dispatch_candidates_reads_still_order() {
    // The over-approximation is LIVE, not theoretical: only `Anvil`'s `weight`
    // reads `ONLY_ANVIL`, and `TOTAL` only ever instantiates `Feather` — yet the
    // read is ordered, because dispatch candidates are followed wholesale. This
    // pins the behavior §5(b) accepted, so a later narrowing is a deliberate
    // decision rather than a silent drift.
    let js = compile(
        r#"
        import std::io::print;
        trait Weight { fun weight(self): i32; }
        struct Feather {}
        struct Anvil {}
        impl Feather with Weight { fun weight(self): i32 { 1 } }
        impl Anvil with Weight { fun weight(self): i32 { ONLY_ANVIL } }
        fun total<T: Weight>(item: T): i32 { item.weight() }
        let TOTAL: i32 = total(Feather {});
        let ONLY_ANVIL: i32 = 99;
        fun main() { print(TOTAL); print(Anvil {}.weight()); }
        "#,
    )
    .expect("clean compile");
    assert!(
        declaration_position(&js, "const ONLY_ANVIL = 99;")
            < declaration_position(&js, "const TOTAL ="),
        "every dispatch candidate's reads order, reachable in this instance or not:\n{js}"
    );
}

// --- B33 S2: the initialization-cycle diagnostic (§3) -----------------------
//
// A dependency cycle among module-level initializers has no valid declaration
// order, so it is a compile error rather than the load-time
// `Cannot access 'B' before initialization` it produced through S1. One
// diagnostic per cycle (not per member), anchored at a read that closes it,
// carrying a `via` chain and the participants' declarations.

#[test]
fn two_bindings_that_read_each_other_are_an_initialization_cycle() {
    // B33 S2 pin 2 — the smallest true cycle, with the chain text asserted.
    assert_fails_with(
        r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = A + 2;
        fun main() { print(A); print(B); }
        "#,
        "`A` and `B` form an initialization cycle: module-level bindings initialize in \
         dependency order, and a cycle has no such order",
    );
    assert_fails_with(
        r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = A + 2;
        fun main() { print(A); print(B); }
        "#,
        "via `A` → `B` → `A`",
    );
}

#[test]
fn a_two_binding_cycle_is_spanned_at_the_read_that_closes_it() {
    // The anchor (diagnostics-standard A1/A3): the read of `B` inside the
    // canonically FIRST member's initializer — not the `let`, not the second
    // member's read, and not a function of enumeration order.
    let source = r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = A + 2;
        fun main() { print(A); print(B); }
        "#;
    // The first `B` in the source is the read inside `A`'s initializer.
    assert_fails_spanning(source, "B", "form an initialization cycle");
}

#[test]
fn a_two_binding_cycle_notes_the_other_declaration() {
    // The C3 note: the read is anchored, and the binding it names is declared
    // over here. (For a cross-module cycle this is what carries the second
    // file — see `a_cross_module_cycle_is_reported_in_the_module_that_reads`.)
    assert_fails_noting(
        r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = A + 2;
        fun main() { print(A); print(B); }
        "#,
        "form an initialization cycle",
        // The declaration span stops before the `;`.
        "let B: i32 = A + 2",
        "`B` is declared here",
    );
}

#[test]
fn a_three_binding_cycle_renders_the_whole_round_trip() {
    // The chain is a real path, not a pair: three members, one diagnostic, and
    // every participant named once. The `via` walk is rooted at the
    // canonically first member and takes the shortest way back to it.
    let diagnostics = failure_diagnostics(
        r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = C + 2;
        let C: i32 = A + 3;
        fun main() { print(A); print(B); print(C); }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "one diagnostic per cycle: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0]
            .0
            .contains("`A`, `B` and `C` form an initialization cycle"),
        "every participant is named: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].0.contains("via `A` → `B` → `C` → `A`"),
        "the chain is the whole round trip: {diagnostics:#?}"
    );
}

#[test]
fn a_cycle_closed_through_a_load_time_call_is_reported() {
    // B33 S2 pin 4 — §2's transitive half. `A`'s initializer CALLS a function
    // that reads `B`; `B`'s initializer reads `A`. Neither initializer names
    // the other binding directly, so only the load-time relation sees this —
    // and the anchor lands on the read inside the callee, which is the read
    // that closes the cycle.
    let source = r#"
        import std::io::print;
        fun read_b(): i32 { B * 2 }
        let A: i32 = read_b() + 1;
        let B: i32 = A + 2;
        fun main() { print(A); print(B); }
        "#;
    assert_fails_with(source, "`A` and `B` form an initialization cycle");
    assert_fails_with(source, "via `A` → `B` → `A`");
    // The first `B` in the source is the one inside `read_b`'s body.
    assert_fails_spanning(source, "B", "form an initialization cycle");
}

#[test]
fn a_cycle_closed_through_a_closure_held_by_a_global_is_reported() {
    // The other transitive shape (§2's "call through a value"): the call goes
    // through a binding holding a closure, whose body reads the cycle's other
    // member. `FETCH` itself is not a participant — it is only entered.
    let diagnostics = failure_diagnostics(
        r#"
        import std::io::print;
        let FETCH: || i32 = || { B };
        let A: i32 = FETCH();
        let B: i32 = A + 2;
        fun main() { print(A); print(B); }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "one diagnostic per cycle: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0]
            .0
            .contains("`A` and `B` form an initialization cycle"),
        "the cycle is between A and B: {diagnostics:#?}"
    );
    assert!(
        !diagnostics[0].0.contains("FETCH"),
        "a binding merely entered on the way is not a participant: {diagnostics:#?}"
    );
}

#[test]
fn a_cross_module_cycle_is_reported_in_the_module_that_reads() {
    // B33 S2 pin 3 — the cross-module cycle: `alpha`'s `A` reads `zeta`'s `Z`
    // and back. The chain names both, the declarations line names both FILES,
    // and the diagnostic is attributed to `alpha.vl` — the file holding the
    // read that closes the cycle — with the span of that read, which is what
    // the editor publishes it against.
    let alpha = "import pkg::zeta::{ Z };\nlet A: i32 = Z + 1;\n";
    let outcome = analyze_package(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::alpha::{ A };\nimport pkg::zeta::{ Z };\n\
                 fun main() { print(A); print(Z); }\n",
            ),
            ("alpha.vl", alpha),
            (
                "zeta.vl",
                "import pkg::alpha::{ A };\nlet Z: i32 = A + 2;\n",
            ),
        ],
        "main.vl",
    );
    assert!(
        outcome.javascript.is_none(),
        "a cross-module cycle does not compile"
    );
    assert_eq!(
        outcome.diagnostics.len(),
        1,
        "one diagnostic per cycle: {:#?}",
        outcome.diagnostics
    );
    let (message, span, file) = &outcome.diagnostics[0];
    assert!(
        message.contains("`A` and `Z` form an initialization cycle"),
        "both members are named: {message}"
    );
    assert!(
        message.contains("via `A` → `Z` → `A`"),
        "the chain names both: {message}"
    );
    assert!(
        message.contains("declared: `A` in `alpha.vl`, `Z` in `zeta.vl`"),
        "each participant's declaration site is named: {message}"
    );
    assert_eq!(
        file.as_deref(),
        Some("alpha.vl"),
        "the diagnostic belongs to the file with the closing read: {message}"
    );
    let read = alpha.find("Z + 1").expect("the read is in alpha.vl");
    assert_eq!(
        *span,
        read..read + 1,
        "spanned at the read of `Z` in alpha.vl: {message}"
    );
}

#[test]
fn a_cycle_is_the_only_diagnostic_however_often_its_members_are_used() {
    // B33 S2 pin 8 — no cascade (diagnostics-standard B5). The members are read
    // from several places, including a function and another binding; the cycle
    // is reported once and nothing downstream of it produces a second error.
    let diagnostics = failure_diagnostics(
        r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = A + 2;
        let USES: i32 = A + B;
        fun consume(): i32 { A + B + USES }
        fun main() { print(A); print(B); print(USES); print(consume()); }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "exactly one diagnostic for one cycle: {diagnostics:#?}"
    );
}

#[test]
fn an_analysis_error_suppresses_the_cycle_check() {
    // The composition rule, pinned so it is a decision and not an accident:
    // the check runs only on a program that analyzed cleanly (the `const` pass
    // takes the same stance, and diagnostics-standard B5 keeps one root cause
    // on screen). The relation is read out of the call graph, which a failed
    // analysis can leave partial — a cycle invented out of half-resolved data
    // would be worse than one reported on the next round. Fixing the type error
    // surfaces the cycle, which the pins above cover.
    let diagnostics = failure_diagnostics(
        r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = A + 2;
        let BROKEN: i32 = "not a number";
        fun main() { print(A); print(B); print(BROKEN); }
        "#,
    );
    assert_eq!(diagnostics.len(), 1, "one root cause: {diagnostics:#?}");
    assert!(
        !diagnostics[0].0.contains("initialization cycle"),
        "the analysis error is the one reported: {diagnostics:#?}"
    );
}

#[test]
fn two_independent_cycles_report_one_diagnostic_each_in_canonical_order() {
    // Per cycle, not per member and not per program: two disjoint cycles are
    // two diagnostics, ordered by their first member's canonical key (which is
    // declaration order here) — deterministic, per diagnostics-standard C1.
    let diagnostics = failure_diagnostics(
        r#"
        import std::io::print;
        let A: i32 = B + 1;
        let B: i32 = A + 2;
        let Y: i32 = Z + 1;
        let Z: i32 = Y + 2;
        fun main() { print(A); print(B); print(Y); print(Z); }
        "#,
    );
    assert_eq!(diagnostics.len(), 2, "one per cycle: {diagnostics:#?}");
    assert!(
        diagnostics[0]
            .0
            .contains("`A` and `B` form an initialization cycle"),
        "the canonically first cycle is reported first: {diagnostics:#?}"
    );
    assert!(
        diagnostics[1]
            .0
            .contains("`Y` and `Z` form an initialization cycle"),
        "then the second: {diagnostics:#?}"
    );
}

#[test]
fn a_cycle_through_a_const_binding_cannot_form() {
    // `const`-marked initializers fold before any of this and contribute no
    // edges (S1's pin 6/12), so a "cycle" written through one is not a cycle:
    // the const chain is a compile-time evaluation, with its own diagnostic if
    // it is circular. Guards against the cycle check inheriting an edge class
    // the ordering relation deliberately does not have.
    assert_compiles(
        r#"
        import std::io::print;
        let STEP: i32 = const 6;
        let DOUBLE: i32 = STEP * 2;
        fun main() { print(DOUBLE); }
        "#,
    );
}

/// Analyzes `source` on a large-stack worker and reports how many
/// `CallGraph::build` calls the whole analysis made. The counter is
/// thread-local and zeroed on the worker, so a concurrently running test
/// cannot contribute to it.
fn call_graphs_built_by_one_analysis(source: &str) -> usize {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            vilan_core::call_graph::reset_build_count();
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            let program = program.expect("analysis should produce a program");
            // The tail's consumers read the installed graph; touching it here
            // must not add a build, which is half of what this counts.
            let _ = program.call_graph();
            vilan_core::call_graph::build_count()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

#[test]
fn one_call_graph_per_analysis() {
    // E35 (`const-eval.md` §8.4). Async inference, platform coloring, const
    // evaluation, the cycle check, chunk planning and emission each used to
    // build their OWN graph over the same tables; they now share the one
    // `post_analysis_passes` builds. Only a counter can pin this: a stale
    // rebuild and a shared build produce identical output whenever the sharing
    // is correct, so no behaviour test can see the difference.
    assert_eq!(
        call_graphs_built_by_one_analysis(
            r#"
            import std::io::print;
            let SEED: i32 = 21;
            let DOUBLE: i32 = double(SEED);
            fun double(value: i32): i32 { value * 2 }
            fun main() { print(DOUBLE); }
            "#
        ),
        1,
        "a program that threads no context must build exactly ONE call graph"
    );
}

/// Analyzes `source` on a large-stack worker and reports how many transformer
/// name seeds the whole analysis built. Same instrument, same reasoning and
/// same isolation as [`call_graphs_built_by_one_analysis`] above.
fn name_seeds_built_by_one_analysis(source: &str) -> usize {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            vilan_core::transformer::reset_name_seed_build_count();
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            let _ = program.expect("analysis should produce a program");
            vilan_core::transformer::name_seed_build_count()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

#[test]
fn the_const_pass_builds_one_name_seed_however_many_const_sites_there_are() {
    // M4 (`const-eval.md` §10). Every `const` site is compiled to its own
    // mini-program, and each one used to start by rebuilding the transformer's
    // name seed — a map of every variable, function and parameter in the
    // reachable world (4,184 entries with `std` loaded), plus the reserved-name
    // set. That is per-site work with a per-analysis answer, and on the
    // website's 210-site entry it was 210 rebuilds. The seed is now built once
    // per pass and shared.
    //
    // Only a counter can pin it: the shared seed produces byte-identical
    // mini-programs, so no behaviour test can distinguish one build from N.
    // Three sites rather than one, because a per-site rebuild reads 1 on a
    // one-site program and would slip through.
    let three_sites = r#"
        import std::io::print;
        let A: i32 = const 1 + 2;
        let B: i32 = const 3 * 4;
        let C: i32 = const 5 - 6;
        fun main() { print(A + B + C); }
        "#;
    assert_eq!(
        name_seeds_built_by_one_analysis(three_sites),
        1,
        "the const pass must build ONE name seed, not one per const site"
    );
    // And the count must not move with the site count — the property that makes
    // the pass linear in its sites rather than linear in sites × program size.
    let six_sites = r#"
        import std::io::print;
        let A: i32 = const 1 + 2;
        let B: i32 = const 3 * 4;
        let C: i32 = const 5 - 6;
        let D: i32 = const 7 + 8;
        let E: i32 = const 9 * 10;
        let F: i32 = const 11 - 12;
        fun main() { print(A + B + C + D + E + F); }
        "#;
    assert_eq!(
        name_seeds_built_by_one_analysis(six_sites),
        name_seeds_built_by_one_analysis(three_sites),
        "doubling the const sites must not change how many name seeds are built"
    );
}

#[test]
fn a_const_site_reads_its_dependency_afresh_in_a_second_analysis() {
    // The memo M4 adds is per-ANALYSIS, and this is what says so: the same
    // process analyzes an edited source and must fold the new value. A seed
    // (or any other const-pass state) that leaked across analyses would fold
    // the first source's answer into the second's program — the failure mode a
    // cache keyed on nothing has.
    let fold = |literal: &str| -> String {
        let source = format!(
            r#"
            import std::io::print;
            let STEP: i32 = {literal};
            let SCALED: i32 = const STEP * 10;
            fun main() {{ print(SCALED); }}
            "#
        );
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                let leaked: &'static str = Box::leak(source.into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    Path::new("."),
                    Path::new("test.vl"),
                    Some(Platform::default()),
                    &Workspace::default(),
                );
                let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
                assert!(
                    messages.is_empty(),
                    "expected a clean analysis, got: {messages:#?}"
                );
                let program = program.expect("analysis should produce a program");
                vilan_core::transform(&program, &vilan_core::BuildOptions::default())
                    .expect("the folded program should emit")
            })
            .expect("spawn worker")
            .join()
            .expect("worker panicked")
    };

    // Both analyses run in THIS process, in this order — the point of the pin.
    let first = fold("4");
    let second = fold("7");
    assert!(
        first.contains("40"),
        "the first analysis should fold `4 * 10`; emitted:\n{first}"
    );
    assert!(
        second.contains("70") && !second.contains("40"),
        "the second analysis must fold the EDITED dependency, not the first \
         analysis's; emitted:\n{second}"
    );
}

/// Analyzes `source` on a large-stack worker and reports how many function
/// bodies the const pass LOWERED. Same instrument, same reasoning and same
/// isolation as [`name_seeds_built_by_one_analysis`] above.
fn const_lowerings_of_one_analysis(source: &str) -> usize {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            vilan_core::transformer::reset_const_lowering_count();
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            let _ = program.expect("analysis should produce a program");
            vilan_core::transformer::const_lowering_count()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

#[test]
fn the_const_pass_lowers_its_world_once_however_many_sites_reach_it() {
    // M4-A (`const-eval.md` §10.6). Every `const` site used to lower the whole
    // function closure its expression reaches into a mini-program of its own —
    // on the website's client entry, 3,873 function emissions across 188 sites
    // for **106** distinct functions. The pass now lowers one world and
    // evaluates every site against it, so the lowering is a fact about the
    // program and not about how many sites read it.
    //
    // Only a counter can pin that: a world lowered once and a world lowered per
    // site produce identical values and identical diagnostics — that is the
    // whole claim — so nothing but the count distinguishes them.
    let sites = |count: usize| {
        let bindings: String = (0..count)
            .map(|index| format!("let SITE{index}: i32 = const doubled({index});\n"))
            .collect();
        let sum: Vec<String> = (0..count).map(|index| format!("SITE{index}")).collect();
        format!(
            r#"
            import std::io::print;
            fun doubled(value: i32): i32 {{ value * 2 }}
            {bindings}
            fun main() {{ print({}); }}
            "#,
            sum.join(" + ")
        )
    };
    let one = const_lowerings_of_one_analysis(&sites(1));
    assert!(
        one > 0,
        "the probe must actually reach a function, or the pin measures nothing"
    );
    assert_eq!(
        const_lowerings_of_one_analysis(&sites(3)),
        one,
        "three sites through one function must lower the same world one site does"
    );
    assert_eq!(
        const_lowerings_of_one_analysis(&sites(6)),
        one,
        "doubling the const sites must not change how much the pass lowers"
    );
}

#[test]
fn const_sites_sharing_a_world_compute_what_they_computed_alone() {
    // The other half of the claim above, stated over values: the sites share a
    // lowering, so they must not share an ANSWER. Each folds its own argument
    // through the same function, and the chain (a site reading a binding another
    // site folded) folds against the shared world too.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        let SCALE: i32 = 10;

        fun scaled(value: i32): i32 {
            value * SCALE
        }

        let A: i32 = const scaled(2);
        let B: i32 = const scaled(3);
        let C: i32 = const scaled(2);
        let CHAINED: i32 = const A + B + C;

        fun main() {
            print(A);
            print(B);
            print(C);
            print(CHAINED);
        }
        main();
        "#,
        "20\n30\n20\n70\n",
    );
}

#[test]
fn a_const_site_reads_its_module_bindings_afresh_and_never_another_sites() {
    // M4-A's isolation property (`const-eval.md` §10.6). What the world shares
    // is the LOWERING — immutable `js::Node` trees, borrowed. Every site still
    // runs in a scope of its own and re-executes its own prelude, so nothing one
    // site's evaluation produced can reach the next one.
    //
    // Two sites take a copy of the same const-folded module binding and grow it;
    // each must see the binding as its initializer left it. Planted to §10.5's
    // literal shape — ONE interpreter scope for the pass, module bindings
    // declared into it once — this reads `TABLE is not defined` and is red. It
    // does NOT redden on a shared scope that still re-declares, because the copy
    // is a copy; that the LEAK itself is unreachable is
    // `no_const_site_can_reach_mutable_module_state_at_all` below, and the two
    // together are why sharing the lowering is safe.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun seed(): List<i32> {
            mut result: List<i32> = List::new();
            result.push(1);
            result
        }

        let TABLE: List<i32> = const seed();

        fun grown(): i32 {
            mut local = TABLE;
            local.push(9);
            local.len()
        }

        let FIRST: i32 = const grown();
        let SECOND: i32 = const grown();

        fun main() {
            print(FIRST);
            print(SECOND);
        }
        main();
        "#,
        "2\n2\n",
    );
}

#[test]
fn no_const_site_can_reach_mutable_module_state_at_all() {
    // Why the sharing above is safe rather than merely observed to be: there is
    // no mutable module-level state a const evaluation can touch, and the two
    // halves of the language close it from both ends. A `mut` module binding is
    // not compile-time-known, so the const pass refuses to reach it; and an
    // immutable one cannot be mutated, so the analyzer refuses that. A prelude
    // therefore only ever declares values built fresh from literals and folded
    // constants (`const-eval.md` §10.6).
    assert_fails_with(
        r#"
        import std::io::print;
        mut COUNTER: i32 = 0;
        fun bump(): i32 {
            COUNTER = COUNTER + 1;
            COUNTER
        }
        let X: i32 = const bump();
        fun main() { print(X); }
        "#,
        "this `const` expression reaches `COUNTER`, whose value is not compile-time-known",
    );
    assert_fails_with(
        r#"
        import std::io::print;
        fun seed(): List<i32> {
            mut result: List<i32> = List::new();
            result.push(1);
            result
        }
        let TABLE: List<i32> = const seed();
        fun grow(): i32 {
            TABLE.push(9);
            TABLE.len()
        }
        let X: i32 = const grow();
        fun main() { print(X); }
        "#,
        "cannot mutate immutable 'TABLE'",
    );
}

#[test]
fn a_const_site_is_refused_only_for_what_it_itself_reaches() {
    // The shared world is one lowering, but a site's REACH is still its own —
    // reconstructed per site from what each emission recorded (`const-eval.md`
    // §10.6). A union would refuse the clean site beside the dirty one, and the
    // multiplicity is the whole point of the pin. Both orders, because a site's
    // reach must not depend on which site the pass evaluated first.
    let capability = |dirty_first: bool| {
        let sites = if dirty_first {
            "let DIRTY: i32 = const impure();\nlet CLEAN: i32 = const doubled(21);"
        } else {
            "let CLEAN: i32 = const doubled(21);\nlet DIRTY: i32 = const impure();"
        };
        format!(
            r#"
            import std::io::print;
            import std::random;
            fun doubled(value: i32): i32 {{ value * 2 }}
            fun impure(): i32 {{ random::range(1, 6) }}
            {sites}
            fun main() {{ print(CLEAN); print(DIRTY); }}
            "#
        )
    };
    assert_fails_once_with(&capability(false), "is not available at expansion time");
    assert_fails_once_with(&capability(true), "is not available at expansion time");

    // And the same for a binding that is not compile-time-known: the site that
    // reaches it is refused, the site beside it folds.
    assert_fails_once_with(
        r#"
        import std::io::print;
        mut COUNTER: i32 = 0;
        fun bump(): i32 { COUNTER = COUNTER + 1; COUNTER }
        fun doubled(value: i32): i32 { value * 2 }
        let DIRTY: i32 = const bump();
        let CLEAN: i32 = const doubled(21);
        fun main() { print(DIRTY); print(CLEAN); }
        "#,
        "whose value is not compile-time-known",
    );
}

#[test]
fn a_const_failure_names_the_function_that_failed_not_the_name_it_was_emitted_under() {
    // §8.2's attribution reads the interpreter's frame trace, which carries
    // EMITTED names — and one name generator now serves the whole pass, so two
    // reached functions that share a source name cannot both be called by it:
    // the second is minted `helper2`, a name no declaration carries. Matching a
    // frame by identity rather than by string is what keeps the diagnostic
    // reading the source (`const-eval.md` §10.6); matching by string drops the
    // subject entirely, which is what this saw before the fix.
    assert_fails_with(
        r#"
        import std::io::print;

        fun table(): List<i32> {
            mut result: List<i32> = List::new();
            result.push(7);
            result
        }

        mod alpha {
            fun helper(values: List<i32>): i32 { values[0] }
        }

        mod beta {
            fun helper(values: List<i32>): i32 { values[3] }
        }

        let GOOD: i32 = const alpha::helper(table());
        let BAD: i32 = const beta::helper(table());

        fun main() { print(GOOD); print(BAD); }
        "#,
        "const evaluation failed in `helper`: index out of bounds",
    );
}

#[test]
fn a_const_function_evaluates_again_after_an_earlier_sites_scopes_were_cleared() {
    // M8 (leak-soak.md §7.8). When a const site's run ends, the interpreter
    // clears every scope the run created — that is what breaks the
    // closure–scope reference cycles. What must survive the teardown is the
    // pass's SHARED LOWERING (`const-eval.md` §10.6): immutable `js::Node`
    // trees, which a later site re-hoists into a fresh scope of its own. So a
    // function two sites reach — including one that declares a function
    // INSIDE its body, the call-scope cycle the root-only experiment in
    // leak-soak.md §7.7 could not break — must evaluate correctly at the
    // later site, after the earlier site's teardown already ran.
    //
    // Planted to a teardown that runs BEFORE the result extraction, the first
    // site cannot read `__const_result` back and this is red.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun bump_twice(value: i32): i32 {
            let bump = |x: i32| { x + 1 };
            bump(bump(value))
        }

        fun scaled(value: i32): i32 {
            value * 3
        }

        let FIRST: i32 = const bump_twice(1);
        let MIDDLE: i32 = const scaled(4);
        let LAST: i32 = const bump_twice(40);

        fun main() {
            print(FIRST);
            print(MIDDLE);
            print(LAST);
        }
        main();
        "#,
        "3\n12\n42\n",
    );
}

/// Analyzes `source` on a large-stack worker and reports the interpreter
/// scopes still alive on that thread afterwards, plus how many function bodies
/// the const pass lowered (the guard that the probe genuinely reached the
/// evaluator). Same isolation as [`const_lowerings_of_one_analysis`].
fn scopes_alive_after_one_analysis(source: &str) -> (isize, usize) {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            vilan_core::transformer::reset_const_lowering_count();
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            let _ = program.expect("analysis should produce a program");
            (
                vilan_core::interpreter::live_scope_count(),
                vilan_core::transformer::const_lowering_count(),
            )
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

#[test]
fn every_interpreter_scope_dies_with_its_const_run() {
    // M8's mechanism pin (leak-soak.md §7.8). A scope that binds a function
    // holds a `Value::Closure` whose `env` is that scope — a cycle `Rc`
    // cannot collect — so before the per-run teardown, every const site of
    // every analysis stranded its root scope, and a function declared inside
    // a body stranded the CALL scope too. Only a counter can pin the fix: the
    // teardown is behaviour-neutral by construction, so nothing observable
    // distinguishes a run that cleans up from one that leaks. The fixture
    // exercises every cycle shape §7.7 names: hoisted module functions (root
    // scope), a closure declared inside a called function (call scope), and
    // loop iterations between them.
    let (alive, lowered) = scopes_alive_after_one_analysis(
        r#"
        import std::io::print;

        fun labels(count: i32): List<str> {
            let describe = |index: i32| { "a labelled entry" };
            mut result: List<str> = List::new();
            mut index = 0;
            for index < count {
                result.push(describe(index));
                index = index + 1;
            }
            result
        }

        fun total(count: i32): i32 {
            mut sum = 0;
            mut index = 0;
            for index < count {
                sum = sum + index;
                index = index + 1;
            }
            sum
        }

        let NAMES: List<str> = const labels(6);
        let SUM: i32 = const total(6);
        let AGAIN: i32 = const total(9);

        fun main() {
            print(NAMES.len());
            print(SUM);
            print(AGAIN);
        }
        main();
        "#,
    );
    assert!(
        lowered > 0,
        "the probe must actually reach the const evaluator, or the pin measures nothing"
    );
    assert_eq!(
        alive, 0,
        "{alive} interpreter scope(s) outlived their const runs — a closure–scope \
         reference cycle is stranding them (leak-soak.md §7.8)"
    );
}

#[test]
fn every_interpreter_scope_dies_with_its_macro_expansion() {
    // The same mechanism pin over the OTHER caller `interpreter.rs` has: macro
    // expansion (`run_entry`), whose world top level hoists functions into the
    // run's root scope exactly as a const site does. The expansion cache keeps
    // only the expansion TEXT, so the run's scopes have nothing left to serve
    // once it returns — the teardown reaches this path too. The macro body is
    // deliberately distinct from every other fixture in this binary, so the
    // process-global expansion cache cannot serve it without running it.
    let (alive, _) = scopes_alive_after_one_analysis(
        r#"
        import std::io::print;

        macro fun sum_to_eleven(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::meta::{ Arguments, Source };
            mut sum = 0;
            mut index = 0;
            for index < 11 {
                sum = sum + index;
                index = index + 1;
            }
            source(i"{sum}")
        }

        fun main() {
            print(macro sum_to_eleven());
        }
        main();
        "#,
    );
    assert_eq!(
        alive, 0,
        "{alive} interpreter scope(s) outlived their macro expansion runs \
         (leak-soak.md §7.8)"
    );
}

#[test]
fn context_threading_owns_the_one_graph_that_cannot_be_shared() {
    // The exception that makes the rule honest. `context::apply` rewrites
    // `entity_map` / `function_calls` / `generic_dispatch` — a threaded `get()`
    // becomes a local read, a consumed `run` becomes `Expr::Null`, and the
    // hidden context argument mints new call entities — so the graph the
    // threading pass planned over does NOT describe the program afterwards and
    // must not be handed on. Such a program pays exactly two builds: the
    // pre-rewrite one, and the tail's.
    //
    // If this ever reads 1, the tail is running on a pre-rewrite graph and is
    // missing the context arguments' edges — a correctness bug, not a saving.
    assert_eq!(
        call_graphs_built_by_one_analysis(
            r#"
            import std::io::print;
            import std::context::Context;

            let flavor: Context<i32> = Context::new();

            fun describe(): str {
                i"flavor {flavor.get()}"
            }

            fun main() {
                print(flavor.run(7, || describe()));
            }
            "#
        ),
        2,
        "a context-threading program must build the pre-rewrite graph AND the tail's"
    );
}

#[test]
fn the_call_graph_is_built_once_and_stays_current() {
    // B33 §4's rider. The cycle check and emission each used to build their own
    // `CallGraph` over the same settled program — ~3% of a clean compile spent
    // twice — so the program now memoizes one and hands it to both
    // (`Program::call_graph`). Two properties keep that honest, and this pins
    // both: the memo is HANDED OUT rather than rebuilt (pointer identity), and
    // it is not STALE — bit-for-bit what a build at emission time produces.
    // Analysis is the only thing that fills those tables; if a pass ever starts
    // rewriting them afterwards, the second assertion is what fails.
    let source = r#"
        import std::io::print;
        let SEED: i32 = 21;
        let DOUBLE: i32 = double(SEED);
        fun double(value: i32): i32 { value * 2 }
        fun main() { print(DOUBLE); }
        "#;
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            let program = program.expect("analysis should produce a program");
            let first = program.call_graph();
            let second = program.call_graph();
            assert!(
                std::ptr::eq(first, second),
                "the call graph is rebuilt per consumer instead of being memoized"
            );
            let fresh = vilan_core::call_graph::CallGraph::build(&program);
            assert_eq!(
                first.debug_dump(&program),
                fresh.debug_dump(&program),
                "the memoized call graph no longer describes the program"
            );
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

// --- Chained element access on a call result (backlog D6, finding 1) ---------
//
// `spec/types.md`, `tour/functions-and-closures.md` and `appendix/gotchas.md`
// all carried a tracked gap: "chained element access on a call result loses the
// element type — bind, then index". The spec's entry claimed "each has a pinned
// test", and for this one no such test existed. All six shapes the D6 audit
// probed compile AND run today, so these pins are what let the claim be deleted
// from the three pages: the trap is dead, and it stays dead by test.

#[test]
fn indexing_a_call_result_keeps_the_element_type() {
    // The gotchas page's own example: `shared.read()[i]`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        import std::shared::Shared;
        fun main() {
            mut backing: List<i32> = List::new();
            backing.push(1);
            backing.push(2);
            let shared = Shared::new(backing);
            print(shared.read()[1]);
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_field_read_through_an_indexed_call_result_keeps_the_element_type() {
    // `rows()[0].name` — the element is a struct, and its field must resolve.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        struct Row { name: str }
        fun rows(): List<Row> {
            mut out: List<Row> = List::new();
            out.push(Row { name = "ada" });
            out
        }
        fun main() {
            print(rows()[0].name);
        }
        "#,
        "ada\n",
    );
}

#[test]
fn a_method_call_on_an_indexed_element_keeps_the_element_type() {
    // `words[1].len()` — the element type must survive to dispatch a method.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        fun main() {
            mut words: List<str> = List::new();
            words.push("a");
            words.push("bcd");
            print(words[1].len());
        }
        "#,
        "3\n",
    );
}

#[test]
fn indexing_a_generic_methods_result_keeps_the_element_type() {
    // `h.all()[1]` — the element type arrives through the impl's binder.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        struct Holder<T> { items: List<T> }
        impl Holder<type T> {
            fun all(self): List<T> {
                self.items
            }
        }
        fun main() {
            mut items: List<i32> = List::new();
            items.push(7);
            items.push(8);
            let holder = Holder { items = items };
            print(holder.all()[1]);
        }
        "#,
        "8\n",
    );
}

#[test]
fn indexing_a_map_value_keeps_the_element_type() {
    // A `List` stored as a `Map` value, indexed after the `Option` unwraps.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        import std::map::Map;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut lists: Map<str, List<i32>> = Map::new();
            mut values: List<i32> = List::new();
            values.push(5);
            lists.insert("k", values);
            match lists.get("k") {
                Some(let l) => {
                    print(l[0]);
                },
                None => {},
            }
        }
        "#,
        "5\n",
    );
}

#[test]
fn indexing_an_indexed_call_result_keeps_the_element_type() {
    // The nested form — `grid()[0][1]`: the inner index must produce a `List`
    // the outer one can index again.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;
        fun grid(): List<List<i32>> {
            mut out: List<List<i32>> = List::new();
            mut inner: List<i32> = List::new();
            inner.push(10);
            inner.push(11);
            out.push(inner);
            out
        }
        fun main() {
            print(grid()[0][1]);
        }
        "#,
        "11\n",
    );
}

// --- Post-`analyze()` diagnostics carry their file (backlog E16) -------------
//
// The passes that run after `analyze()` walk the WHOLE program, so there is no
// "file being walked" to attribute their diagnostics to — before this they all
// defaulted to the entry, which made the editor squiggle the wrong file and the
// CLI render the wrong text. Each now attributes from the anchor entity whose
// span it reports. (`const`, platform coloring and the `[must_use]` warnings are
// pinned end-to-end in `vilan-cli/tests/diagnostics.rs`, where the rendering is
// observable; these are the two that only the attribution channel shows.)

#[test]
fn an_async_divergence_in_a_module_is_attributed_to_the_module() {
    let outcome = analyze_package(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::alpha::go;\nfun main() { print(go()); }\n",
            ),
            (
                "alpha.vl",
                "import std::time::sleep;\n\
                 external fun host_transform(f: |i32| i32): i32;\n\
                 fun go(): i32 {\n\thost_transform(|n| {\n\t\tsleep(1);\n\t\tn\n\t})\n}\n",
            ),
        ],
        "main.vl",
    );
    let (message, _span, file) = outcome
        .diagnostics
        .iter()
        .find(|(message, _, _)| message.contains("cannot await a Vilan closure"))
        .expect("the host-boundary divergence is reported");
    assert_eq!(
        file.as_deref(),
        Some("alpha.vl"),
        "the divergence belongs to the module holding the call: {message}"
    );
}

#[test]
fn an_async_drop_in_a_module_is_attributed_to_the_module() {
    let outcome = analyze_package(
        &[
            (
                "main.vl",
                "import pkg::alpha::make;\nfun main() { let held = make(); }\n",
            ),
            (
                "alpha.vl",
                "import std::drop::Drop;\n\
                 resource struct Res { x: i32 }\n\
                 impl Res with Drop {\n\tasync fun drop(&mut self) {}\n}\n\
                 fun make(): Res { Res { x = 1 } }\n",
            ),
        ],
        "main.vl",
    );
    let (message, _span, file) = outcome
        .diagnostics
        .iter()
        .find(|(message, _, _)| message.contains("teardown must be synchronous"))
        .expect("the async-drop rejection is reported");
    assert_eq!(
        file.as_deref(),
        Some("alpha.vl"),
        "the rejection belongs to the module holding the `drop` body: {message}"
    );
}

// --- E108: an unresolved TYPE name is attributed like its value twin --------
//
// The two positions are one mechanism with one difference: the value site
// attributes its diagnostic to the file the name was walked from and the type
// site did not. Both are raised out of a queue drained in `build()`, long after
// the per-file walks, so an unattributed one keeps whatever `current_source_id`
// the last walk left — std's `lib.vl` — and the span then indexes a comment in
// a file the author has never opened. (The prelude lane's find, Order 22.)

#[test]
fn an_unresolved_type_in_a_module_is_attributed_to_the_module() {
    const ALPHA: &str = "fun make(): Widget {\n\t1\n}\n";
    let outcome = analyze_package(
        &[
            (
                "main.vl",
                "import pkg::alpha::make;\nfun main() { make(); }\n",
            ),
            ("alpha.vl", ALPHA),
        ],
        "main.vl",
    );
    let (message, span, file) = outcome
        .diagnostics
        .iter()
        .find(|(message, _, _)| message.contains("cannot find type 'Widget'"))
        .expect("the unresolved type is reported");
    let start = ALPHA.find("Widget").unwrap();
    assert_eq!(
        (file.as_deref(), span.clone()),
        (Some("alpha.vl"), start..start + "Widget".len()),
        "the annotation belongs to the module that wrote it: {message}"
    );
}

#[test]
fn an_unresolved_value_in_a_module_is_attributed_to_the_module_too() {
    // The control the type case is measured against — green before E108 and
    // after it, which is what makes the pair a claim about ONE mechanism.
    const ALPHA: &str = "fun make(): i32 {\n\twidget_value\n}\n";
    let outcome = analyze_package(
        &[
            (
                "main.vl",
                "import pkg::alpha::make;\nfun main() { make(); }\n",
            ),
            ("alpha.vl", ALPHA),
        ],
        "main.vl",
    );
    let (message, span, file) = outcome
        .diagnostics
        .iter()
        .find(|(message, _, _)| message.contains("cannot find 'widget_value'"))
        .expect("the unresolved value is reported");
    let start = ALPHA.find("widget_value").unwrap();
    assert_eq!(
        (file.as_deref(), span.clone()),
        (Some("alpha.vl"), start..start + "widget_value".len()),
        "the read belongs to the module that wrote it: {message}"
    );
}

#[test]
fn a_bare_trait_annotation_in_a_module_is_attributed_to_the_module() {
    // The other diagnostic the same drain raises, carrying the same defect: an
    // annotation that RESOLVED, to a trait, in value position (§12.2). A FIELD
    // since B186 — the parameter this was written on became the implicit
    // generic, and a field is the nearest position that still refuses.
    const ALPHA: &str = "trait Shape {\n\tfun area(&self): i32;\n}\n\nstruct Holder {\n\tshape: \
                         Shape,\n}\n\nfun size(): i32 {\n\t0\n}\n";
    let outcome = analyze_package(
        &[
            (
                "main.vl",
                "import pkg::alpha::size;\nfun main() { size(); }\n",
            ),
            ("alpha.vl", ALPHA),
        ],
        "main.vl",
    );
    let (message, span, file) = outcome
        .diagnostics
        .iter()
        .find(|(message, _, _)| message.contains("'Shape' is a trait, not a type"))
        .expect("the bare trait in value position is refused");
    let start = ALPHA.find("shape: Shape").unwrap() + "shape: ".len();
    assert_eq!(
        (file.as_deref(), span.clone()),
        (Some("alpha.vl"), start..start + "Shape".len()),
        "the refusal belongs to the module that wrote the annotation: {message}"
    );
}

#[test]
fn an_entry_global_does_not_resolve_through_a_std_module_path() {
    // B52: `path::name` addresses what the namespace DECLARES. The member
    // lookup used to walk the scope chain out to the global scope — where
    // the entry's top-level items live — so any entry global resolved
    // through any std module path (and the chain lookup's memoization then
    // cached the entry id INTO the std scope).
    assert_fails(
        r#"
        import std::math;
        fun helper(): i32 { 7 }
        fun main() { let x = math::helper(); }
        "#,
    );
}

#[test]
fn an_entry_global_does_not_satisfy_a_std_import_path() {
    // B52, the import form of the same hole: the segment walk used the same
    // chain lookup, so `import std::math::<entry global>` resolved.
    assert_fails(
        r#"
        import std::math::helper;
        fun helper(): i32 { 7 }
        fun main() { let x = helper(); }
        "#,
    );
}

// --- B172: a module-qualified path is a type in every type position ----------
//
// `style::Style` used to be a PARSE error wherever a type is written, while
// `style::style()` and `style::Display::Flex` resolved and ran: the type
// grammar's nominal form was a bare `IDENT`, so the `::` never belonged to the
// type and whatever the position demanded next found it instead. That made the
// `std::web` prelude — which carries `style` and `ui` as MODULE names — able to
// reach every VALUE in `std::style` and no TYPE in it, and both web templates
// carried a forced `import std::style::Style;` to work around it.
//
// The positions below are the whole list a type can be written in. Each one is
// its own pin, per file policy: a class of positions closed on one
// representative is how a "closed" item turns out never to have been covered.

/// The module every position pin qualifies through: a struct, a trait it
/// implements, and a function — so a path can name a type, a bound, and a
/// non-type member in the same namespace.
const SHAPES: &str = r#"
        mod shapes {
            trait Named {
                fun name(&self): str;
            }

            struct Dot {
                x: i32,
            }

            impl Dot with Named {
                fun name(&self): str { "dot" }
            }

            fun make(): Dot {
                Dot { x = 1 }
            }
        }
"#;

/// `SHAPES` followed by `rest` — the two-part source every pin below builds.
fn with_shapes(rest: &str) -> String {
    format!("{SHAPES}\n{rest}\n")
}

#[test]
fn a_qualified_path_is_a_return_type() {
    assert_compiles(&with_shapes(
        "fun first(): shapes::Dot { shapes::make() }\n\
         fun main() { let d = first(); print(i\"{d.x}\"); }",
    ));
}

#[test]
fn a_qualified_path_is_a_let_annotation() {
    assert_compiles(&with_shapes(
        "fun main() { let d: shapes::Dot = shapes::make(); print(i\"{d.x}\"); }",
    ));
}

#[test]
fn a_qualified_path_is_a_parameter_type() {
    assert_compiles(&with_shapes(
        "fun width(d: shapes::Dot): i32 { d.x }\n\
         fun main() { print(i\"{width(shapes::make())}\"); }",
    ));
}

#[test]
fn a_qualified_path_is_a_struct_field_type() {
    assert_compiles(&with_shapes(
        "struct Holder {\n\tinner: shapes::Dot,\n}\n\
         fun main() { let h = Holder { inner = shapes::make() }; print(i\"{h.inner.x}\"); }",
    ));
}

#[test]
fn a_qualified_path_is_an_impl_subject() {
    assert_compiles(&with_shapes(
        "impl shapes::Dot {\n\tfun doubled(&self): i32 { self.x * 2 }\n}\n\
         fun main() { print(i\"{shapes::make().doubled()}\"); }",
    ));
}

#[test]
fn a_qualified_path_is_a_trait_bound() {
    assert_compiles(&with_shapes(
        "fun label<T: shapes::Named>(value: &T): str { value.name() }\n\
         fun main() { print(label(&shapes::make())); }",
    ));
}

#[test]
fn a_qualified_path_is_a_generic_argument() {
    assert_compiles(&with_shapes(
        "fun main() { let all: List<shapes::Dot> = [shapes::make()]; print(i\"{all.len()}\"); }",
    ));
}

#[test]
fn a_qualified_path_nests_inside_another_type() {
    // The type grammar is a cycle, so the path form has to be reachable from
    // every arm of it, not just from the position the annotation opened in.
    assert_compiles(&with_shapes(
        "fun main() {\n\
         \tlet nested: List<List<shapes::Dot>> = [[shapes::make()]];\n\
         \tlet viewed: |shapes::Dot| i32 = |d: shapes::Dot| d.x;\n\
         \tlet pair: (shapes::Dot, i32) = (shapes::make(), 2);\n\
         \tlet boxed: [shapes::Dot; 1] = [shapes::make(); 1];\n\
         \tlet seen: &shapes::Dot = &pair.0;\n\
         \tprint(i\"{nested.len()}{viewed(shapes::make())}{boxed.len()}{seen.x}\");\n\
         }",
    ));
}

#[test]
fn a_qualified_path_carries_generic_arguments_on_its_last_segment() {
    // `std::reactive::SignalCell<i32>` — a path of any depth whose tail is a
    // generic application. The arguments parameterize the type the path names,
    // exactly as they do on a bare `SignalCell<i32>`.
    assert_compiles(
        r#"
        import std::reactive;
        fun main() {
            let cell: reactive::SignalCell<i32> = reactive::Signal::new(1);
            print(i"{cell.get()}");
        }
        "#,
    );
}

#[test]
fn a_qualified_path_reaches_through_several_modules() {
    assert_compiles(
        r#"
        mod outer {
            mod inner {
                struct Dot {
                    x: i32,
                }

                fun make(): Dot {
                    Dot { x = 3 }
                }
            }
        }
        fun main() {
            let d: outer::inner::Dot = outer::inner::make();
            print(i"{d.x}");
        }
        "#,
    );
}

#[test]
fn a_qualified_path_to_a_non_type_member_is_refused() {
    // The negative the positive needs: the path resolves — `make` IS in
    // `shapes` — and still is not a type. Before this the member's own type
    // (a closure) was written into the annotation's slot and the mistake
    // surfaced, if at all, as a mismatch at the initializer.
    assert_fails_with(
        &with_shapes("fun main() { let d: shapes::make = 1; print(i\"{d}\"); }"),
        "'make' in module 'shapes' is not a type",
    );
}

#[test]
fn a_qualified_path_to_a_missing_member_is_refused() {
    assert_fails_with(
        &with_shapes("fun main() { let d: shapes::Blob = 1; print(i\"{d}\"); }"),
        "cannot find 'Blob' in module 'shapes'",
    );
}

#[test]
fn a_qualified_path_through_a_non_module_is_refused() {
    assert_fails_with(
        &with_shapes("fun main() { let d: shapes::Dot::Inner = 1; print(i\"{d}\"); }"),
        "is not a module",
    );
}

#[test]
fn a_qualified_path_refusal_is_attributed_to_the_module_that_wrote_it() {
    // E108's rule, extended to the drain this item gave diagnostics to. These
    // are raised in `build()`, after every per-file walk, so an unattributed
    // push keeps whatever `current_source_id` the last walk left — std's
    // `lib.vl` — and the span then indexes a file the author never opened.
    const ALPHA: &str = "mod shapes {\n\
        \tfun make(): i32 {\n\
        \t\t1\n\
        \t}\n\
        }\n\n\
        fun size(): shapes::make {\n\
        \t0\n\
        }\n";
    let outcome = analyze_package(
        &[
            (
                "main.vl",
                "import pkg::alpha::size;\nfun main() { size(); }\n",
            ),
            ("alpha.vl", ALPHA),
        ],
        "main.vl",
    );
    let (message, span, file) = outcome
        .diagnostics
        .iter()
        .find(|(message, _, _)| message.contains("'make' in module 'shapes' is not a type"))
        .expect("the non-type path member is refused");
    let start = ALPHA.find("shapes::make").unwrap();
    assert_eq!(
        (file.as_deref(), span.clone()),
        (Some("alpha.vl"), start..start + "shapes::make".len()),
        "the refusal belongs to the module that wrote the annotation: {message}"
    );
}

// The web-prelude half of B172 — that a module-carried name reaches its TYPES
// as well as its values — needs a manifest prelude, so it is pinned beside the
// prelude harness in `tests/module_resolution.rs`.

// --- B190: a struct LITERAL takes the same qualified head ---------------------
//
// B172 admitted `type-path` in every TYPE position and left one spelling
// behind: the literal, whose rule keyed on a bare identifier followed by `{`.
// So `shapes::Dot` was a type, `shapes::make()` was a call, and
// `shapes::Dot { x = 1 }` was a PARSE error ("expected `;` to end this
// statement") — which is why two of B172's own pins construct through a
// `make()` helper instead of saying what they mean. The literal now reads the
// same production, and the condition-position rule is untouched: a condition
// parses through the no-struct mode, which never reaches the literal rule at
// all, so a qualified path before a `{` there stays an operand exactly as the
// bare form does.

/// A module with a nested module, an enum and a struct — enough to write a
/// literal head of one segment, of two, and one that is not a struct at all.
const QUALIFIED: &str = r#"
        mod shapes {
            import std::compare::PartialEq;

            mod deep {
                struct Ring {
                    r: i32,
                }
            }

            enum Kind {
                Round(i32),
                Flat,
            }

            impl Kind with PartialEq {
                fun eq(self, other: Kind): bool {
                    if self is Kind::Flat {
                        other is Kind::Flat
                    } else {
                        false
                    }
                }
            }

            struct Dot {
                x: i32,
            }
        }
"#;

fn with_qualified(rest: &str) -> String {
    format!("{QUALIFIED}\n{rest}\n")
}

#[test]
fn a_qualified_struct_literal_takes_one_segment() {
    assert_compiles_and_runs(
        &with_qualified(
            "fun main() { let d = shapes::Dot { x = 1 }; print(i\"{d.x}\"); }\nmain();",
        ),
        "1\n",
    );
}

#[test]
fn a_qualified_struct_literal_takes_two_segments() {
    // The production is a repetition, not a special case for one `::`, so the
    // nested module has to work for the same reason `std::reactive::SignalCell`
    // works as a type.
    assert_compiles_and_runs(
        &with_qualified(
            "fun main() { let r = shapes::deep::Ring { r = 2 }; print(i\"{r.r}\"); }\nmain();",
        ),
        "2\n",
    );
}

#[test]
fn a_qualified_literal_head_that_is_not_a_struct_is_refused_by_name_not_by_the_parser() {
    // The enum-variant twin. This language's variants carry a POSITIONAL
    // payload (`Kind::Round(1)`), so there is no such thing as
    // `Kind::Round { r = 1 }` — but the mistake is now a semantic one, told in
    // the vocabulary of what the path names, where before the parser refused
    // the whole statement with "expected `;`" and said nothing about `Round`.
    // The path walks a module and then an enum, whose namespace holds its
    // variants exactly as a `use` statement reads it.
    let source = with_qualified("fun main() { let k = shapes::Kind::Round { r = 1 }; }\nmain();");
    assert_fails_with(
        &source,
        "cannot initialize a non-struct: shapes::Kind::Round",
    );
    assert_fails_without(&source, "expected `;`");
}

#[test]
fn an_unknown_qualified_literal_head_names_the_path_as_written() {
    // The miss reports the spelling the author used, not the last segment on
    // its own — `Nope` alone would send them looking in the wrong file.
    assert_fails_with(
        &with_qualified("fun main() { let n = shapes::Nope { x = 1 }; }\nmain();"),
        "unknown struct: shapes::Nope",
    );
}

#[test]
fn a_qualified_path_before_a_brace_in_condition_position_is_still_an_operand() {
    // The disambiguation, at the spelling the change introduces. A condition's
    // operands exclude struct literals so that `if Foo { … }` is a block; the
    // qualified form has to obey the same rule, or every `if x == mod::Enum::V
    // { … }` in the language would start reading its own body as a field list.
    assert_compiles_and_runs(
        &with_qualified(
            "fun main() {\n\
             \tlet k = shapes::Kind::Flat;\n\
             \tif k == shapes::Kind::Flat { print(\"flat\"); }\n\
             \tfor _ in [1] { print(\"once\"); }\n\
             }\nmain();",
        ),
        "flat\nonce\n",
    );
}

#[test]
fn a_parenthesised_qualified_literal_is_admitted_in_a_condition() {
    // The escape the spec names for the bare form, which the qualified form
    // inherits unchanged: parenthesise the literal and the condition takes it.
    assert_compiles_and_runs(
        &with_qualified(
            "fun main() { if (shapes::Dot { x = 1 }).x == 1 { print(\"one\"); } }\nmain();",
        ),
        "one\n",
    );
}
