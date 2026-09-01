//! Declared bounds (B12), view invalidation, reactive turns and the optimistic
//! lifecycle, async closure types, string forms, and `const` evaluation with
//! its asset channel.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- B12: a generic bound instantiated at a type LACKING the impl must be a ---
// --- spanned compile error, not a silent dispatch to the abstract member.  ---

// The shared shape: `Dog` implements `Greet`, `Cat` does not. `greet` returns
// void so a miss is the fully SILENT miscompile (no return-type error to trip
// over) — the worst form of the class.
const GREET_PRELUDE: &str = r#"
    trait Greet {
        fun greet(self);
    }
    struct Dog { name: str }
    struct Cat { name: str }
    impl Dog with Greet {
        fun greet(self) {
            let _woof = self.name;
        }
    }
"#;

#[test]
fn a_bound_satisfied_by_an_impl_still_compiles() {
    assert_compiles(&format!(
        r#"{GREET_PRELUDE}
        fun describe<T: Greet>(subject: T) {{
            subject.greet();
        }}
        fun main() {{
            describe(Dog {{ name = "rex" }});
        }}
        main();
        "#
    ));
}

#[test]
fn a_free_function_bound_rejects_a_type_without_the_impl() {
    let source = format!(
        r#"{GREET_PRELUDE}
        fun describe<T: Greet>(subject: T) {{
            subject.greet();
        }}
        fun main() {{
            describe(Cat {{ name = "tom" }});
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        r#"describe(Cat { name = "tom" })"#,
        "does not implement trait 'Greet'",
    );
}

#[test]
fn a_method_own_generic_bound_rejects_a_type_without_the_impl() {
    let source = format!(
        r#"{GREET_PRELUDE}
        struct Kennel {{ size: i32 }}
        impl Kennel {{
            fun admit<T: Greet>(self, guest: T) {{
                guest.greet();
            }}
        }}
        fun main() {{
            let kennel = Kennel {{ size = 3 }};
            kennel.admit(Cat {{ name = "tom" }});
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        r#"kennel.admit(Cat { name = "tom" })"#,
        "does not implement trait 'Greet'",
    );
}

#[test]
fn a_multi_bound_names_the_missing_trait() {
    // `Dog` implements `Greet` but not `Fetch` — the error must name `Fetch`.
    let source = format!(
        r#"{GREET_PRELUDE}
        trait Fetch {{
            fun fetch(self);
        }}
        fun train<T: Greet + Fetch>(subject: T) {{
            subject.greet();
            subject.fetch();
        }}
        fun main() {{
            train(Dog {{ name = "rex" }});
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        r#"train(Dog { name = "rex" })"#,
        "does not implement trait 'Fetch'",
    );
}

#[test]
fn a_static_bound_call_rejects_a_type_without_the_impl() {
    // The `T::member()` channel: an explicit generic argument that fails the bound.
    let source = format!(
        r#"{GREET_PRELUDE}
        trait Fresh {{
            fun fresh(): Self;
        }}
        impl Dog with Fresh {{
            fun fresh(): Self {{
                ret Dog {{ name = "pup" }};
            }}
        }}
        fun spawn<T: Fresh>(): T {{
            ret T::fresh();
        }}
        fun main() {{
            let _cat: Cat = spawn<Cat>();
        }}
        main();
        "#
    );
    assert_fails(&source);
}

#[test]
fn a_rebounded_forward_still_compiles() {
    // A wrapper that re-declares the bound forwards legally.
    assert_compiles(&format!(
        r#"{GREET_PRELUDE}
        fun describe<T: Greet>(subject: T) {{
            subject.greet();
        }}
        fun relay<U: Greet>(subject: U) {{
            describe(subject);
        }}
        fun main() {{
            relay(Dog {{ name = "rex" }});
        }}
        main();
        "#
    ));
}

#[test]
fn a_generic_impl_subject_satisfies_the_bound() {
    // `impl Crate2<type X> with Greet` covers every `Crate2<..>` instantiation.
    assert_compiles(&format!(
        r#"{GREET_PRELUDE}
        struct Crate2<T> {{ inner: T }}
        impl Crate2<type X> with Greet {{
            fun greet(self) {{
                let _hi = 1;
            }}
        }}
        fun describe<T: Greet>(subject: T) {{
            subject.greet();
        }}
        fun main() {{
            describe(Crate2 {{ inner = 5 }});
        }}
        main();
        "#
    ));
}

#[test]
fn a_trait_default_without_an_impl_does_not_satisfy_the_bound() {
    // A default body is inherited THROUGH an impl; with no `impl Cat with
    // Chatty` at all, the bound stays unsatisfied.
    let source = r#"
        trait Chatty {
            fun chat(self) {
                let _hello = 1;
            }
        }
        struct Cat { name: str }
        fun engage<T: Chatty>(subject: T) {
            subject.chat();
        }
        fun main() {
            engage(Cat { name = "tom" });
        }
        main();
        "#;
    assert_fails_spanning(
        source,
        r#"engage(Cat { name = "tom" })"#,
        "does not implement trait 'Chatty'",
    );
}

#[test]
fn an_under_bounded_forward_is_rejected_at_the_inner_call() {
    // Forwarding through a wrapper does NOT launder the requirement: the
    // wrapper's own parameter must re-declare the bound (see
    // `a_rebounded_forward_still_compiles` for the legal spelling).
    let source = format!(
        r#"{GREET_PRELUDE}
        fun describe<T: Greet>(subject: T) {{
            subject.greet();
        }}
        fun outer<U>(x: U) {{
            describe(x);
        }}
        fun main() {{
            outer(Dog {{ name = "rex" }});
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        "describe(x)",
        "generic parameter 'U' is missing the bound ': Greet'",
    );
}

#[test]
fn a_bound_satisfied_through_a_subtrait_impl_compiles() {
    // Implementing a SUBTRAIT satisfies a supertrait bound: `Loud` extends
    // `Greet`, and `impl Dog with Loud` must satisfy `T: Greet`.
    assert_compiles(
        r#"
        trait Greet {
            fun greet(self);
        }
        trait Loud with Greet {
            fun shout(self);
        }
        struct Dog { name: str }
        impl Dog with Loud {
            fun greet(self) {
                let _quiet = 1;
            }
            fun shout(self) {
                let _loud = 2;
            }
        }
        fun describe<T: Greet>(subject: T) {
            subject.greet();
        }
        fun main() {
            describe(Dog { name = "rex" });
        }
        main();
        "#,
    );
}

// --- B12 depth: a CONDITIONAL impl (`impl Box2<type X: Greet> with Greet`) ---
// --- satisfies a bound only when its binder bounds hold at the argument.   ---

const CONDITIONAL_PRELUDE: &str = r#"
    trait Greet {
        fun greet(self);
    }
    struct Dog { name: str }
    struct Cat { name: str }
    impl Dog with Greet {
        fun greet(self) {
            let _woof = self.name;
        }
    }
    struct Box2<T> { inner: T }
    impl Box2<type X: Greet> with Greet {
        fun greet(self) {
            self.inner.greet();
        }
    }
    fun describe<T: Greet>(subject: T) {
        subject.greet();
    }
"#;

#[test]
fn a_conditional_impl_with_a_satisfied_condition_compiles() {
    assert_compiles(&format!(
        r#"{CONDITIONAL_PRELUDE}
        fun main() {{
            describe(Box2 {{ inner = Dog {{ name = "rex" }} }});
        }}
        main();
        "#
    ));
}

#[test]
fn a_conditional_impl_with_a_failed_condition_is_rejected() {
    let source = format!(
        r#"{CONDITIONAL_PRELUDE}
        fun main() {{
            describe(Box2 {{ inner = Cat {{ name = "tom" }} }});
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        r#"describe(Box2 { inner = Cat { name = "tom" } })"#,
        "does not implement trait 'Greet'",
    );
}

#[test]
fn a_conditional_impl_checks_recursively() {
    // The condition applies at every level: a box of boxes of dogs greets,
    // a box of boxes of cats does not.
    assert_compiles(&format!(
        r#"{CONDITIONAL_PRELUDE}
        fun main() {{
            describe(Box2 {{ inner = Box2 {{ inner = Dog {{ name = "rex" }} }} }});
        }}
        main();
        "#
    ));
    let source = format!(
        r#"{CONDITIONAL_PRELUDE}
        fun main() {{
            describe(Box2 {{ inner = Box2 {{ inner = Cat {{ name = "tom" }} }} }});
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        r#"describe(Box2 { inner = Box2 { inner = Cat { name = "tom" } } })"#,
        "does not implement trait 'Greet'",
    );
}

#[test]
fn an_inherited_binder_bound_conditions_the_impl() {
    // The impl binder declares no bound of its own, so it INHERITS the struct
    // declaration's (`struct Kennel2<T: Greet>`); binding through the impl
    // must still enforce it.
    let source = r#"
        trait Greet {
            fun greet(self);
        }
        trait Show {
            fun show(self);
        }
        struct Dog { name: str }
        struct Cat { name: str }
        impl Dog with Greet {
            fun greet(self) {
                let _woof = self.name;
            }
        }
        struct Kennel2<T: Greet> { inner: T }
        impl Kennel2<type T> with Show {
            fun show(self) {
                self.inner.greet();
            }
        }
        fun display<T: Show>(subject: T) {
            subject.show();
        }
        fun main() {
            display(Kennel2 { inner = Cat { name = "tom" } });
        }
        main();
        "#;
    assert_fails_spanning(
        source,
        r#"display(Kennel2 { inner = Cat { name = "tom" } })"#,
        "does not implement trait 'Show'",
    );
}

// --- B12 family: DECLARED bounds check at CONSTRUCTION — a struct literal ---
// --- or enum-variant call binding a declared generic must satisfy it.     ---

#[test]
fn a_struct_literal_satisfying_the_declared_bound_compiles() {
    assert_compiles(&format!(
        r#"{GREET_PRELUDE}
        struct Kennel2<T: Greet> {{ inner: T }}
        fun main() {{
            let _kennel = Kennel2 {{ inner = Dog {{ name = "rex" }} }};
        }}
        main();
        "#
    ));
}

#[test]
fn a_struct_literal_violating_the_declared_bound_is_rejected() {
    let source = format!(
        r#"{GREET_PRELUDE}
        struct Kennel2<T: Greet> {{ inner: T }}
        fun main() {{
            let _kennel = Kennel2 {{ inner = Cat {{ name = "tom" }} }};
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        r#"Kennel2 {{ inner = Cat {{ name = "tom" }} }}"#
            .replace("{{", "{")
            .replace("}}", "}")
            .as_str(),
        "does not implement trait 'Greet'",
    );
}

#[test]
fn an_enum_variant_violating_the_declared_bound_is_rejected() {
    let source = format!(
        r#"{GREET_PRELUDE}
        enum Slot<T: Greet> {{
            Filled(T),
            Empty,
        }}
        fun main() {{
            let _slot = Slot::Filled(Cat {{ name = "tom" }});
        }}
        main();
        "#
    );
    assert_fails(&source);
}

#[test]
fn an_enum_variant_satisfying_the_declared_bound_compiles() {
    assert_compiles(&format!(
        r#"{GREET_PRELUDE}
        enum Slot<T: Greet> {{
            Filled(T),
            Empty,
        }}
        fun main() {{
            let _slot = Slot::Filled(Dog {{ name = "rex" }});
        }}
        main();
        "#
    ));
}

#[test]
fn a_generic_struct_literal_with_a_bounded_forward_compiles() {
    // Construction inside a generic function whose parameter re-declares the
    // bound is legal.
    assert_compiles(&format!(
        r#"{GREET_PRELUDE}
        struct Kennel2<T: Greet> {{ inner: T }}
        fun pack<U: Greet>(value: U) {{
            let _kennel = Kennel2 {{ inner = value }};
        }}
        fun main() {{
            pack(Dog {{ name = "rex" }});
        }}
        main();
        "#
    ));
}

// The unbounded-forward gap's root fix: the initializer's second-chance
// FIELD-first reconcile binds a declared parameter from a generic field
// value, so the argument reads as the caller's `U` (whose missing bound the
// declared-bound check then rejects) instead of the constraint fallback.
#[test]
fn a_generic_struct_literal_with_an_unbounded_forward_is_rejected() {
    let source = format!(
        r#"{GREET_PRELUDE}
        struct Kennel2<T: Greet> {{ inner: T }}
        fun pack<U>(value: U) {{
            let _kennel = Kennel2 {{ inner = value }};
        }}
        fun main() {{
            pack(Dog {{ name = "rex" }});
        }}
        main();
        "#
    );
    assert_fails(&source);
}

#[test]
fn a_partially_binding_variant_still_checks_its_bound_parameter() {
    // `Pair::Left` binds only `A` — the check must still fire on `A` even
    // though `B` stays unbound at this construction.
    let source = format!(
        r#"{GREET_PRELUDE}
        enum Pair<A: Greet, B: Greet> {{
            Left(A),
            Right(B),
        }}
        fun main() {{
            let _left = Pair::Left(Cat {{ name = "tom" }});
        }}
        main();
        "#
    );
    assert_fails(&source);
}

// --- B12 family: bound trait ARGUMENTS must match — an impl providing ---
// --- `Feed<str>` does not satisfy `F: Feed<i32>`.                     ---

const FEED_PRELUDE: &str = r#"
    trait Feed<T> {
        fun feed(self, food: T);
    }
    struct Bird { name: str }
    struct Fish { name: str }
    impl Bird with Feed<str> {
        fun feed(self, food: str) {
            let _crumbs = food;
        }
    }
    impl Fish with Feed<i32> {
        fun feed(self, food: i32) {
            let _flakes = food;
        }
    }
"#;

#[test]
fn a_matching_trait_argument_satisfies_the_bound() {
    assert_compiles(&format!(
        r#"{FEED_PRELUDE}
        fun wants_numbers<F: Feed<i32>>(feeder: F) {{
            feeder.feed(3);
        }}
        fun main() {{
            wants_numbers(Fish {{ name = "bubbles" }});
        }}
        main();
        "#
    ));
}

#[test]
fn a_mismatched_trait_argument_is_rejected() {
    let source = format!(
        r#"{FEED_PRELUDE}
        fun wants_numbers<F: Feed<i32>>(feeder: F) {{
            feeder.feed(3);
        }}
        fun main() {{
            wants_numbers(Bird {{ name = "tweety" }});
        }}
        main();
        "#
    );
    assert_fails_spanning(
        &source,
        r#"wants_numbers(Bird { name = "tweety" })"#,
        "does not implement trait 'Feed<i32>'",
    );
}

#[test]
fn a_bound_argument_flowing_from_another_generic_is_checked() {
    // `F: Feed<T>` with `T` bound by a sibling argument: eat(bird, 5) needs
    // Feed<i32>, and Bird only provides Feed<str>.
    assert_compiles(&format!(
        r#"{FEED_PRELUDE}
        fun eat<T, F: Feed<T>>(feeder: F, seed: T) {{
            feeder.feed(seed);
        }}
        fun main() {{
            eat(Bird {{ name = "tweety" }}, "worm");
        }}
        main();
        "#
    ));
    let source = format!(
        r#"{FEED_PRELUDE}
        fun eat<T, F: Feed<T>>(feeder: F, seed: T) {{
            feeder.feed(seed);
        }}
        fun main() {{
            eat(Bird {{ name = "tweety" }}, 5);
        }}
        main();
        "#
    );
    assert_fails(&source);
}

#[test]
fn a_declared_bound_trait_argument_is_checked_at_construction() {
    let source = format!(
        r#"{FEED_PRELUDE}
        struct Aviary<F: Feed<i32>> {{ feeder: F }}
        fun main() {{
            let _aviary = Aviary {{ feeder = Bird {{ name = "tweety" }} }};
        }}
        main();
        "#
    );
    assert_fails(&source);
}

#[test]
fn a_conditional_impl_binder_trait_argument_is_checked() {
    // The binder bound carries arguments too: a box is only numeric-feedable
    // when its content feeds on numbers.
    let source = format!(
        r#"{FEED_PRELUDE}
        struct Box3<T> {{ inner: T }}
        impl Box3<type X: Feed<i32>> with Feed<i32> {{
            fun feed(self, food: i32) {{
                self.inner.feed(food);
            }}
        }}
        fun wants_numbers<F: Feed<i32>>(feeder: F) {{
            feeder.feed(3);
        }}
        fun main() {{
            wants_numbers(Box3 {{ inner = Bird {{ name = "tweety" }} }});
        }}
        main();
        "#
    );
    assert_fails(&source);
}

#[test]
fn a_generic_enum_variant_with_an_unbounded_forward_is_rejected() {
    // The enum analogue of the struct forward: the checker derives the
    // variant's bindings by reconciling payload types against argument
    // types, so the caller's unbounded `U` surfaces and fails the bound.
    let source = format!(
        r#"{GREET_PRELUDE}
        enum Slot<T: Greet> {{
            Filled(T),
            Empty,
        }}
        fun pack<U>(value: U) {{
            let _slot = Slot::Filled(value);
        }}
        fun main() {{
            pack(Dog {{ name = "rex" }});
        }}
        main();
        "#
    );
    assert_fails(&source);
}

#[test]
fn a_generic_enum_variant_with_a_bounded_forward_compiles() {
    assert_compiles(&format!(
        r#"{GREET_PRELUDE}
        enum Slot<T: Greet> {{
            Filled(T),
            Empty,
        }}
        fun pack<U: Greet>(value: U) {{
            let _slot = Slot::Filled(value);
        }}
        fun main() {{
            pack(Dog {{ name = "rex" }});
        }}
        main();
        "#
    ));
}

// --- view-invalidation.md E2: a mutating call on the viewed root is an ---
// --- invalidating event, like reassignment (rule 4).                   ---

#[test]
fn a_mutating_method_under_a_live_element_view_is_rejected() {
    // The proposal's P3: pop() may drop the viewed element.
    let source = r#"
        fun main() {
            mut a = [ 0 ];
            let b = &mut a[0];
            a.pop();
            b = 99;
        }
        main();
        "#;
    assert_fails_spanning(
        source,
        "a.pop()",
        "cannot mutate 'a' with '.pop(..)' while a view into it is live",
    );
}

#[test]
fn a_push_under_a_live_element_view_is_rejected() {
    // push is included deliberately: harmless on JS, reallocates on native.
    let source = r#"
        fun main() {
            mut a = [ 0 ];
            let b = &mut a[0];
            a.push(1);
            b = 99;
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn passing_the_viewed_root_by_mut_ref_is_rejected() {
    // The proposal's P4: the callee may resize the container.
    let source = r#"
        fun grow(list: &mut List<i32>) {
            list.push(7);
        }
        fun main() {
            mut a = [ 0 ];
            let b = &mut a[0];
            grow(&mut a);
            b = 99;
        }
        main();
        "#;
    assert_fails_spanning(
        source,
        "grow(&mut a)",
        "cannot pass '&mut a' to 'grow' while a view into it is live",
    );
}

#[test]
fn a_user_mut_self_method_under_a_live_view_is_rejected() {
    let source = r#"
        struct Basket { items: List<i32> }
        impl Basket {
            fun clear_items(&mut self) {
                self.items = [];
            }
        }
        fun main() {
            mut basket = Basket { items = [ 1 ] };
            let held = &mut basket.items;
            basket.clear_items();
            held.push(2);
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn a_read_only_method_under_a_live_view_compiles() {
    // &self methods do not invalidate.
    assert_compiles(
        r#"
        import std::io::print;
        fun main() {
            mut a = [ 5 ];
            let b = &mut a[0];
            print(a.len());
            b = 99;
        }
        main();
        "#,
    );
}

#[test]
fn writing_through_the_view_itself_compiles() {
    // The view's whole purpose; not an invalidating event.
    assert_compiles(
        r#"
        fun main() {
            mut a = [ 5 ];
            let b = &mut a[0];
            b = 99;
            b = 100;
        }
        main();
        "#,
    );
}

#[test]
fn mutating_an_unrelated_container_compiles() {
    assert_compiles(
        r#"
        fun main() {
            mut a = [ 5 ];
            mut other = [ 1 ];
            let b = &mut a[0];
            other.pop();
            b = 99;
        }
        main();
        "#,
    );
}

#[test]
fn a_mutating_call_before_the_view_exists_compiles() {
    // Scan order: the view is not yet live at the call.
    assert_compiles(
        r#"
        fun main() {
            mut a = [ 5 ];
            a.pop();
            a.push(6);
            let b = &mut a[0];
            b = 99;
        }
        main();
        "#,
    );
}

#[test]
fn a_mutating_call_in_a_nested_block_under_an_outer_view_is_rejected() {
    // Lexical liveness carries into inner blocks.
    let source = r#"
        fun main() {
            mut a = [ 0 ];
            let b = &mut a[0];
            {
                a.pop();
            }
            b = 99;
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn mutating_the_container_inside_a_for_mut_loop_is_rejected() {
    // The loop binding is a view into the container for the body's extent.
    let source = r#"
        fun main() {
            mut a = [ 1, 2, 3 ];
            for e in &mut a {
                a.pop();
            }
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn reassigning_the_container_inside_a_for_mut_loop_is_rejected() {
    // The same loop-binding origin feeds the shipped E1 (reassignment) check.
    let source = r#"
        fun main() {
            mut a = [ 1, 2, 3 ];
            for e in &mut a {
                a = [];
            }
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn a_mut_call_on_a_viewed_scalar_root_compiles() {
    // The transparent-references demo's shape: a scalar's boxed cell has no
    // geometry — a callee can only write the slot, which is the aliasing the
    // model permits. E2 exempts scalar roots.
    assert_compiles(
        r#"
        import std::io::print;
        fun add_ten(value: &mut i32) {
            value += 10;
        }
        fun main() {
            mut a: i32 = 10;
            let b: &mut i32 = &mut a;
            add_ten(&mut a);
            print(*b);
        }
        main();
        "#,
    );
}

// --- view-invalidation.md E3: a view may not live across `await` — the ---
// --- writer set during a suspension is the whole program.              ---

#[test]
fn a_view_across_await_is_rejected() {
    // The proposal's probe program (compiled silently before E3).
    let source = r#"
        struct Point { x: i32 }
        async fun tick() {
            let _beat = 1;
        }
        async fun mutate_across_await() {
            mut point = Point { x = 1 };
            let view = &mut point;
            await tick();
            view.x = 99;
        }
        fun main() {
            mutate_across_await();
        }
        main();
        "#;
    assert_fails_spanning(source, "await tick()", "cannot hold a view across 'await'");
}

#[test]
fn a_view_created_after_the_await_compiles() {
    assert_compiles(
        r#"
        struct Point { x: i32 }
        async fun tick() {
            let _beat = 1;
        }
        async fun late_view() {
            mut point = Point { x = 1 };
            await tick();
            let view = &mut point;
            view.x = 99;
        }
        fun main() {
            late_view();
        }
        main();
        "#,
    );
}

#[test]
fn an_await_in_one_branch_under_a_live_view_is_rejected() {
    // Lexical liveness: an await on ANY path while the view is live counts.
    let source = r#"
        struct Point { x: i32 }
        async fun tick() {
            let _beat = 1;
        }
        async fun branchy(flag: bool) {
            mut point = Point { x = 1 };
            let view = &mut point;
            if flag {
                await tick();
            }
            view.x = 99;
        }
        fun main() {
            branchy(true);
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn an_await_inside_a_for_mut_loop_is_rejected() {
    // The loop binding is a view live across every iteration.
    let source = r#"
        async fun tick() {
            let _beat = 1;
        }
        async fun stream() {
            mut items = [ 1, 2, 3 ];
            for e in &mut items {
                await tick();
            }
        }
        fun main() {
            stream();
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn a_shared_write_view_across_await_is_rejected() {
    // The settled sub-question: Shared is NOT exempt — the handle pins the
    // cell (memory-safe), but another turn's write still reseats elements
    // under the held view. Re-acquire after the await. (`read()` returns a
    // COPY by design, so only `write()`'s view is at stake — see the guard
    // below.)
    let source = r#"
        import std::shared::Shared;
        async fun tick() {
            let _beat = 1;
        }
        async fun stale_view() {
            let shared = Shared::new([ 1, 2, 3 ]);
            let list = shared.write();
            await tick();
            list.push(4);
        }
        fun main() {
            stale_view();
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn a_shared_read_copy_across_await_compiles() {
    // `read()` returns a copy (value semantics) — nothing to invalidate.
    assert_compiles(
        r#"
        import std::shared::Shared;
        import std::io::print;
        async fun tick() {
            let _beat = 1;
        }
        async fun fresh_copy() {
            let shared = Shared::new([ 1, 2, 3 ]);
            let list = shared.read();
            await tick();
            print(list.len());
        }
        fun main() {
            fresh_copy();
        }
        main();
        "#,
    );
}

#[test]
fn an_async_function_with_a_view_parameter_is_rejected() {
    // The signature rule: the caller's view would be held inside the
    // suspended callee across its awaits.
    let source = r#"
        async fun tick() {
            let _beat = 1;
        }
        async fun stash(value: &mut i32) {
            await tick();
            value += 1;
        }
        fun main() {
            mut a = 5;
            stash(&mut a);
        }
        main();
        "#;
    assert_fails_spanning(source, "value", "cannot take '&mut' parameters");
}

// The `&` half of the same signature rule (`view-invalidation.md` §3: "an `async
// fun` may not declare `&`/`&mut` parameters"). Only the `&mut` spelling above
// was ever pinned — B112's survey found the gap. A SHARED view is no safer than
// a mutable one here: the hazard is the caller's claim outliving its epoch while
// the callee sits suspended, and reading through a stale view is the read half
// of exactly that. The message names the form it saw, so the two spellings are
// separately observable.

#[test]
fn an_async_function_with_a_shared_view_parameter_is_rejected() {
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick() {
            let _beat = 1;
        }
        async fun peek(viewed: &Point) {
            await tick();
            let _seen = viewed.x;
        }
        fun main() {
            let point = Point { x = 1, y = 2 };
            peek(&point);
        }
        main();
        "#;
    assert_fails_spanning(source, "viewed", "cannot take '&' parameters");
}

#[test]
fn an_async_method_with_a_shared_view_receiver_is_rejected() {
    // A receiver is an ordinary parameter with a `&` convention, so `&self`
    // meets the same rule and anchors on the `self` token.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick() {
            let _beat = 1;
        }
        impl Point {
            async fun peek(&self) {
                await tick();
                let _seen = self.x;
            }
        }
        fun main() {
            let point = Point { x = 1, y = 2 };
            point.peek();
        }
        main();
        "#;
    assert_fails_spanning(source, "self", "cannot take '&' parameters");
}

// --- B119: the view gate asks the CALL GRAPH, not the `await` token ---------
//
// Filed 2026-08-10 probing the `&` form; the bug was the GATE's, not the
// form's — both spellings escaped. The signature rule fired only when the body
// held an explicit `await`, so the IMPLICIT-await spelling
// (`spec/execution.md` §7: calling an async function without the keyword, the
// sanctioned form) bypassed it entirely, and the emission proved the
// suspension real: `const beat = await (tick());` with the caller's view live
// across it.
//
// Ruled 2026-08-12: a call gates the view rule when it CAN SUSPEND — the
// callee is declared `async`, or is transitively suspending. The token stays
// SUFFICIENT (every pin above is untouched); it stops being NECESSARY.
// Declared asyncness is read of the CALLEE at a call site, never of the body
// being checked — which is what keeps B29's freedom below intact rather than
// merely tolerated.
#[test]
fn an_implicit_await_does_not_lift_the_async_view_parameter_rule() {
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        async fun stash(viewed: &mut Point) {
            let beat = tick();
            viewed.x = beat;
        }
        fun main() {
            mut point = Point { x = 1, y = 2 };
            stash(&mut point);
        }
        main();
        "#;
    assert_fails_spanning(source, "viewed", "cannot take '&mut' parameters");
}

#[test]
fn an_implicit_await_does_not_lift_the_shared_view_parameter_rule() {
    // The `&` spelling of the filed shape — the same gate, the other form.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        async fun peek(viewed: &Point) {
            let beat = tick();
            let _seen = viewed.x + beat;
        }
        fun main() {
            let point = Point { x = 1, y = 2 };
            peek(&point);
        }
        main();
        "#;
    assert_fails_spanning(source, "viewed", "cannot take '&' parameters");
}

#[test]
fn an_implicit_await_through_a_sync_declared_hop_is_still_a_suspension() {
    // The transitive half of the rule: `hop` declares nothing, but it calls
    // `tick`, so `async_infer` makes it async and `stash` awaits it —
    // `return await (tick());` inside `hop`, `await (hop())` inside `stash`.
    // A gate keyed on the callee's DECLARATION would miss this entirely; the
    // fixpoint's is what sees it.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        fun hop(): i32 { tick() }
        async fun stash(viewed: &mut Point) {
            let beat = hop();
            viewed.x = beat;
        }
        fun main() {
            mut point = Point { x = 1, y = 2 };
            stash(&mut point);
        }
        main();
        "#;
    assert_fails_spanning(source, "viewed", "cannot take '&mut' parameters");
}

#[test]
fn an_implicit_await_refuses_every_view_parameter_of_the_body() {
    // Multi-parameter: one suspension point condemns the whole signature, and
    // each form anchors at its own parameter with its own spelling.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        fun stash(viewed: &mut Point, other: &Point) {
            let beat = tick();
            viewed.x = beat + other.y;
        }
        fun main() {
            mut point = Point { x = 1, y = 2 };
            let seen = Point { x = 3, y = 4 };
            stash(&mut point, &seen);
        }
        main();
        "#;
    assert_fails_spanning(source, "viewed", "cannot take '&mut' parameters");
    assert_fails_spanning(source, "other", "cannot take '&' parameters");
}

#[test]
fn a_view_across_an_implicit_await_in_the_body_is_rejected() {
    // E3's BODY rule had the identical hole, and the same answer closes it.
    // The diagnostic anchors at the CALL — there is no token to point at — and
    // names it, so the reader can see which call suspends.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        async fun flow() {
            mut point = Point { x = 1, y = 2 };
            let view = &mut point;
            let beat = tick();
            view.x = beat;
        }
        fun main() { flow(); }
        main();
        "#;
    // Occurrence 1 — occurrence 0 is `tick()` inside the declaration itself.
    assert_fails_spanning_nth(source, "tick()", 1, "the implicit 'await' of 'tick'");
    assert_fails_with(source, "'view' (a view into 'point') is still live here");
}

#[test]
fn an_async_closure_capturing_a_view_across_an_implicit_await_is_rejected() {
    // E3's CLOSURE rule, third arm of the same gate.
    let source = r#"
        async fun tick(): i32 { 1 }
        fun main() {
            mut a = 5;
            let view = &mut a;
            let task = async {
                let beat = tick();
                view = beat;
            };
        }
        main();
        "#;
    assert_fails_with(source, "an async closure cannot capture the view 'view'");
}

#[test]
fn an_explicit_await_reports_its_crossing_exactly_once() {
    // The token path and the call-site path both see `await tick()` — the
    // awaited call is the token's own suspension written out. Only the token
    // reports it, or every existing `await` diagnostic would have doubled.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        async fun flow() {
            mut point = Point { x = 1, y = 2 };
            let view = &mut point;
            let beat = await tick();
            view.x = beat;
        }
        fun main() { flow(); }
        main();
        "#;
    let crossings = failure_diagnostics(source)
        .into_iter()
        .filter(|(message, _)| message.contains("cannot hold a view across"))
        .count();
    assert_eq!(crossings, 1, "expected exactly one crossing diagnostic");
}

#[test]
fn a_body_with_both_await_spellings_reports_its_signature_once() {
    // Mixed: the token settles the signature rule, so the call-site path never
    // records a candidate for this body and the parameter is named once.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        async fun stash(viewed: &mut Point) {
            let beat = await tick();
            let more = tick();
            viewed.x = beat + more;
        }
        fun main() {
            mut point = Point { x = 1, y = 2 };
            stash(&mut point);
        }
        main();
        "#;
    let refusals = failure_diagnostics(source)
        .into_iter()
        .filter(|(message, _)| message.contains("cannot take '&mut' parameters"))
        .count();
    assert_eq!(refusals, 1, "expected exactly one signature diagnostic");
}

#[test]
fn a_declared_async_body_that_never_suspends_keeps_its_view_parameters() {
    // THE composition with B29, at function granularity. The gate asks what
    // the BODY does, never what the declaration says: a JS `async function`
    // runs synchronously to its first `await`, so a body with no suspension
    // point never yields and its view parameter is safe for exactly as long as
    // it is the body's own. Tightening the gate to declared asyncness — the
    // reading the filing warned against — would redden this and, with it,
    // `a_declared_async_impl_of_a_sync_trait_method_is_permitted`.
    assert_compiles(
        r#"
        struct Point { x: i32, y: i32 }
        async fun quiet(viewed: &mut Point) {
            viewed.x = 5;
        }
        fun main() {
            mut point = Point { x = 1, y = 2 };
            quiet(&mut point);
        }
        main();
        "#,
    );
}

#[test]
fn calling_an_async_impl_of_a_sync_trait_method_suspends_the_caller() {
    // The other side of the same composition, and the RUNTIME truth decides
    // it: B29 lets `S`'s `m` be `async` under a sync trait declaration, and
    // `async_infer` propagates that through the contract — the emission is
    // `await (m(s))` inside `caller`. An `await` yields to the microtask queue
    // even for an already-resolved promise, so `caller`'s views really are
    // held across a suspension and the rule must say so. B29's own pin never
    // calls `m`, which is why both hold.
    let source = r#"
        trait T { fun m(&self): void; }
        struct S {}
        impl S with T { async fun m(&self): void {} }
        struct Point { x: i32, y: i32 }
        fun caller(viewed: &mut Point, s: &S) {
            s.m();
            viewed.x = 5;
        }
        fun main() {
            mut point = Point { x = 1, y = 2 };
            let s = S {};
            caller(&mut point, &s);
        }
        main();
        "#;
    assert_fails_spanning(source, "viewed", "cannot take '&mut' parameters");
}

#[test]
fn a_sync_call_with_a_view_live_across_it_stays_legal() {
    // The NEGATIVE. Nothing in this program can suspend, so a view lives
    // across the call untouched — the rule must not fire on a call merely for
    // being a call.
    assert_compiles(
        r#"
        struct Point { x: i32, y: i32 }
        fun beat(): i32 { 7 }
        fun flow() {
            mut point = Point { x = 1, y = 2 };
            let view = &mut point;
            let value = beat();
            view.x = value;
        }
        fun main() { flow(); }
        main();
        "#,
    );
}

#[test]
fn a_sync_call_inside_an_async_body_keeps_a_live_view_legal() {
    // The negative again, one frame in: an async CALLER is not a suspension —
    // only the suspension points inside it are — so a purely sync call with a
    // view live across it compiles inside an `async fun` too.
    assert_compiles(
        r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        fun beat(): i32 { 7 }
        async fun flow() {
            let _warm = tick();
            mut point = Point { x = 1, y = 2 };
            let view = &mut point;
            let value = beat();
            view.x = value;
        }
        fun main() { flow(); }
        main();
        "#,
    );
}

#[test]
fn a_view_declared_after_an_implicit_await_stays_legal() {
    // Ordering: liveness runs from the declaration, so a suspension BEFORE the
    // view is not a crossing. The same conservatism E3 has always had, now
    // applied to the implicit spelling.
    assert_compiles(
        r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        async fun flow() {
            let beat = tick();
            mut point = Point { x = beat, y = 2 };
            let view = &mut point;
            view.x = 9;
        }
        fun main() { flow(); }
        main();
        "#,
    );
}

#[test]
fn an_implicit_await_nested_in_a_branch_still_crosses_a_live_view() {
    // Nested: the suspension is inside an `if` arm, the view outside it.
    let source = r#"
        struct Point { x: i32, y: i32 }
        async fun tick(): i32 { 1 }
        async fun flow(flag: bool) {
            mut point = Point { x = 1, y = 2 };
            let view = &mut point;
            if flag {
                let beat = tick();
                view.x = beat;
            }
        }
        fun main() { flow(true); }
        main();
        "#;
    assert_fails_with(source, "cannot hold a view across the implicit 'await'");
}

#[test]
fn an_implicit_await_in_a_for_mut_loop_crosses_the_loop_binding() {
    // Nested, and the view is the LOOP BINDING — live across every iteration.
    let source = r#"
        async fun tick(): i32 { 1 }
        async fun stream() {
            mut items = [ 1, 2, 3 ];
            for e in &mut items {
                let _beat = tick();
            }
        }
        fun main() { stream(); }
        main();
        "#;
    assert_fails_with(source, "cannot hold a view across the implicit 'await'");
}

#[test]
fn an_implicit_await_of_a_method_names_the_method_in_the_crossing() {
    // The diagnostic's voice on a receiver call: the callee is resolved
    // through the wired subject, so the message names `tick`, not the
    // receiver.
    let source = r#"
        struct Clock { ticks: i32 }
        impl Clock {
            async fun tick(&self): i32 { 1 }
        }
        struct Point { x: i32, y: i32 }
        async fun flow(clock: Clock) {
            mut point = Point { x = 1, y = 2 };
            let view = &mut point;
            let beat = clock.tick();
            view.x = beat;
        }
        fun main() { flow(Clock { ticks = 0 }); }
        main();
        "#;
    assert_fails_with(source, "the implicit 'await' of 'tick'");
}

#[test]
fn a_sync_function_with_view_parameters_called_from_async_compiles() {
    // Sync callees cannot suspend — views pass freely.
    assert_compiles(
        r#"
        async fun tick() {
            let _beat = 1;
        }
        fun bump(value: &mut i32) {
            value += 1;
        }
        async fun flow() {
            mut a = 5;
            bump(&mut a);
            await tick();
            bump(&mut a);
        }
        fun main() {
            flow();
        }
        main();
        "#,
    );
}

#[test]
fn an_async_closure_capturing_a_view_is_rejected() {
    let source = r#"
        async fun tick() {
            let _beat = 1;
        }
        fun main() {
            mut a = 5;
            let view = &mut a;
            let task = async {
                await tick();
                view += 1;
            };
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn an_await_with_no_live_views_compiles() {
    assert_compiles(
        r#"
        async fun tick() {
            let _beat = 1;
        }
        async fun clean() {
            mut a = [ 1 ];
            a.push(2);
            await tick();
            a.push(3);
        }
        fun main() {
            clean();
        }
        main();
        "#,
    );
}

// --- K2: the std math surface (proposal: backlog K2) ---

#[test]
fn math_constants_and_moved_free_functions_import() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::math::{ PI, TAU, E, EPSILON, min, max, minmax };

        fun main() {
            print(PI);
            print(TAU == PI * 2f);
            print(E > 2.7f && E < 2.8f);
            print(EPSILON > 0f);
            print(min(3, 9));
            print(max(3, 9));
            let (low, high) = minmax(9, 3);
            print(low);
            print(high);
        }
        main();
        "#,
        "3.141592653589793\ntrue\ntrue\ntrue\n3\n9\n3\n9\n",
    );
}

#[test]
fn f64_float_classification_predicates() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::math::{ NAN, INFINITY };

        fun main() {
            print(NAN.is_nan());
            print(1.5f.is_nan());
            print(1.5f.is_finite());
            print(INFINITY.is_finite());
            print(INFINITY.is_infinite());
            print(NAN.is_infinite());
        }
        main();
        "#,
        "true\nfalse\ntrue\nfalse\ntrue\nfalse\n",
    );
}

#[test]
fn rem_is_truncated_remainder_across_the_families() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print(7.rem(3));
            print((0 - 7).rem(3));
            print(7.5f.rem(2f));
            print(250u8.rem(7u8));
            print(9i53.rem(4i53));
        }
        main();
        "#,
        "1\n-1\n1.5\n5\n1\n",
    );
}

#[test]
fn sized_types_carry_the_applicable_math_family() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print((0i8 - 5i8).abs());
            print(3i16.pow(2i16));
            print(200u16.min(90u16));
            print(7u53.max(9u53));
            print(2f32.pow(3f32));
            print(2.25f32.sqrt());
        }
        main();
        "#,
        "5\n9\n90\n9\n8\n1.5\n",
    );
}

// --- K2 side-fix: conformance credits a SEPARATE impl of the declaring ---
// --- supertrait (impl X with Eq {} need not restate PartialEq's eq).   ---

#[test]
fn a_marker_impl_rides_a_separate_supertrait_impl() {
    assert_compiles(
        r#"
        trait Alike<B = Self> {
            fun same(self, b: B): bool;
        }
        trait Settled with Alike {}
        struct Coin { face: i32 }
        impl Coin with Alike {
            fun same(self, b: Coin): bool {
                self.face == b.face
            }
        }
        impl Coin with Settled {}
        fun main() {
            let _ok = Coin { face = 1 }.same(Coin { face = 1 });
        }
        main();
        "#,
    );
}

#[test]
fn a_missing_supertrait_member_still_errors() {
    let source = r#"
        trait Alike<B = Self> {
            fun same(self, b: B): bool;
        }
        trait Settled with Alike {}
        struct Coin { face: i32 }
        impl Coin with Settled {}
        fun main() {
            let _coin = Coin { face = 1 };
        }
        main();
        "#;
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(message, _)| message
            .contains("'Coin' does not implement trait 'Settled': missing 'same'")),
        "got: {diagnostics:#?}"
    );
}

#[test]
fn a_same_named_member_from_an_unrelated_trait_does_not_satisfy() {
    // `same` provided via an UNRELATED trait's impl must not satisfy
    // `Settled`'s inherited requirement.
    let source = r#"
        trait Alike<B = Self> {
            fun same(self, b: B): bool;
        }
        trait Settled with Alike {}
        trait Lookalike {
            fun same(self, b: Self): bool;
        }
        struct Coin { face: i32 }
        impl Coin with Lookalike {
            fun same(self, b: Coin): bool {
                self.face == b.face
            }
        }
        impl Coin with Settled {}
        fun main() {
            let _coin = Coin { face = 1 };
        }
        main();
        "#;
    assert_fails(source);
}

// --- reactive-turns §5.1: `get_safe` — the possibly-established context ---
// --- read (ambient-owner.md §2.1's sketch; turn_scope's prerequisite).  ---

#[test]
fn get_safe_yields_none_outside_and_some_inside_a_run() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ Some, None };

        let current: Context<i32> = Context::new();

        fun describe(): str {
            match current.get_safe() {
                Some(let value) => i"some {value}",
                None => "none",
            }
        }

        fun main() {
            print(describe());
            current.run(7, || {
                print(describe());
            });
            print(describe());
        }
        main();
        "#,
        "none\nsome 7\nnone\n",
    );
}

#[test]
fn get_safe_wraps_inside_a_strict_covered_region() {
    // A strict (get-reading) function calls a safe-only one: the boundary
    // Some-wraps the bare value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ Some, None };

        let current: Context<i32> = Context::new();

        fun peek(): str {
            match current.get_safe() {
                Some(let value) => i"peeked {value}",
                None => "nothing",
            }
        }

        fun strict_report() {
            let value = current.get();
            print(i"strict {value}");
            print(peek());
        }

        fun main() {
            current.run(9, || {
                strict_report();
            });
        }
        main();
        "#,
        "strict 9\npeeked 9\n",
    );
}

#[test]
fn get_safe_threads_through_a_transitive_chain() {
    // The middle function neither reads nor runs — the Option threads
    // through it, Some on the covered path and None from the top level.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ Some, None };

        let current: Context<i32> = Context::new();

        fun leaf(): str {
            match current.get_safe() {
                Some(let value) => i"leaf {value}",
                None => "leaf none",
            }
        }

        fun middle(): str {
            leaf()
        }

        fun main() {
            print(middle());
            current.run(3, || {
                print(middle());
            });
        }
        main();
        "#,
        "leaf none\nleaf 3\n",
    );
}

#[test]
fn get_safe_survives_await_and_stored_closures() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ Some, None };

        let current: Context<i32> = Context::new();

        fun label(): str {
            match current.get_safe() {
                Some(let value) => i"got {value}",
                None => "got none",
            }
        }

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            mut stored: List<|| void> = [];
            current.run(5, || {
                let task = async {
                    await tick();
                    print(label());
                };
                stored.push(|| print(label()));
            });
            print(label());
            for callback in stored {
                callback();
            }
        }
        main();
        "#,
        "got none\ngot 5\ngot 5\n",
    );
}

#[test]
fn the_strict_fence_is_unchanged_by_get_safe() {
    // A strict `get` on an uncovered path still errors, even in a program
    // that also uses `get_safe`; and a get_safe-only function pulled onto a
    // strict chain is fenced like any strict code.
    let source = r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ Some, None };

        let current: Context<i32> = Context::new();

        fun sneaky(): i32 {
            current.get()
        }

        fun probe(): str {
            match current.get_safe() {
                Some(let value) => i"some {value}",
                None => "none",
            }
        }

        fun main() {
            print(probe());
            print(sneaky());
        }
        main();
        "#;
    assert_fails_spanning(
        source,
        "current.get()",
        "can be reached without an enclosing `run`",
    );
}

// --- reactive-turns §5.2: turn-scoped flush — the isolation model. ---

#[test]
fn a_turn_flush_cannot_drain_another_turns_queue() {
    // The two-requests scenario, distilled: B's flush must not fire A's
    // pending notification.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Turn, turn_scope, flush };

        fun main() {
            let a = Signal::new(0);
            let _watch = a.sub(|value| print(i"a {value}"));
            let turn_a = Turn::new();
            let turn_b = Turn::new();
            turn_scope.run(turn_a, || {
                a.set(1);
            });
            turn_scope.run(turn_b, || flush());
            print("b flushed");
            turn_scope.run(turn_a, || flush());
        }
        main();
        "#,
        "a 0\nb flushed\na 1\n",
    );
}

#[test]
fn a_batch_body_defers_even_at_the_top_level() {
    // The batch body is INJECTED (created before the extent exists), so its
    // writes defer to batch's own fresh turn — the shipped batch semantics,
    // now per-extent instead of a global depth counter.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, batch };

        fun main() {
            let count = Signal::new(0);
            let _watch = count.sub(|value| print(i"seen {value}"));
            batch(|| {
                count.set(1);
                count.set(2);
                print("settling");
            });
        }
        main();
        "#,
        "seen 0\nsettling\nseen 2\n",
    );
}

#[test]
fn a_turn_follows_its_extents_continuation_across_await() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Turn, turn_scope, flush };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let count = Signal::new(0);
            let _watch = count.sub(|value| print(i"seen {value}"));
            let mine = Turn::new();
            turn_scope.run(mine, || {
                let task = async {
                    await tick();
                    count.set(7);
                    flush();
                };
            });
            print("sync done");
        }
        main();
        "#,
        "seen 0\nsync done\nseen 7\n",
    );
}

// --- reactive-turns §2: the UI event boundary mechanism — a host-invoked ---
// --- plain ADAPTER wraps each dispatch in a fresh turn; the clause-typed ---
// --- handler (a user literal, deferred) receives it at the call.        ---

#[test]
fn a_host_invoked_adapter_gives_each_dispatch_its_own_turn() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, FlushPolicy, turn, turn_scope };

        fun simulate_events(handler: (|| void) context turn_scope) {
            // The DOM stores only this plain closure; each invocation is a
            // boundary dispatch.
            let adapter = || turn(FlushPolicy::AtSuspension, || handler());
            adapter();
            adapter();
        }

        fun main() {
            let count = Signal::new(0);
            let _watch = count.sub(|value| print(i"seen {value}"));
            simulate_events(|| {
                count.set(count.get() + 1);
                count.set(count.get() + 1);
                print("handler done");
            });
        }
        main();
        "#,
        "seen 0\nhandler done\nseen 2\nhandler done\nseen 4\n",
    );
}

#[test]
fn a_named_handler_binding_adopts_the_clause() {
    // `let add = || ..; take(add)` — the unannotated closure binding passed
    // into a clause position adopts it: the literal defers (receiving each
    // dispatch's turn), and DIRECT calls of the binding thread like any
    // injected call.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, FlushPolicy, turn, turn_scope };

        fun dispatch(handler: (|| void) context turn_scope) {
            turn(FlushPolicy::AtEnd, || handler());
        }

        fun main() {
            let count = Signal::new(0);
            let _watch = count.sub(|value| print(i"seen {value}"));
            let add = || {
                count.set(count.get() + 1);
                count.set(count.get() + 1);
            };
            dispatch(add);
            print("mid");
            turn(FlushPolicy::AtEnd, || add());
        }
        main();
        "#,
        "seen 0\nseen 2\nmid\nseen 4\n",
    );
}

#[test]
fn an_annotated_clause_binding_defers_and_forwards() {
    // The explicit spelling: a clause on the LET annotation. The binding
    // forwards into same-clause parameters and works as `run`'s body.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let current: Context<i32> = Context::new();

        fun invoke(body: (|| void) context current) {
            current.run(9, body);
        }

        fun main() {
            let report: (|| void) context current = || print(current.get());
            invoke(report);
            current.run(5, report);
        }
        main();
        "#,
        "9\n5\n",
    );
}

#[test]
fn a_non_closure_binding_in_a_clause_position_is_rejected() {
    let source = r#"
        import std::reactive::{ FlushPolicy, turn, turn_scope };

        fun dispatch(handler: (|| void) context turn_scope) {
            turn(FlushPolicy::AtEnd, || handler());
        }

        fun main() {
            let not_a_closure = 5;
            dispatch(not_a_closure);
        }
        main();
        "#;
    assert_fails(source);
}

#[test]
fn an_annotated_binding_with_a_non_literal_initializer_is_rejected() {
    let source = r#"
        import std::context::Context;

        let current: Context<i32> = Context::new();

        fun main() {
            let value = 5;
            let bad: (|| void) context current = value;
        }
        main();
        "#;
    assert_fails(source);
}

// --- reactive-turns: the suspension hook. A turn's async continuations ---
// --- must settle without manual flushes, and AtSuspension pre-flushes  ---
// --- at each await (the optimistic-paint cadence).                     ---

#[test]
fn a_continuation_set_settles_without_a_manual_flush() {
    // The silent-loss fix: after the extent's first suspension the turn is
    // SETTLED; a late enqueue drains itself instead of waiting forever.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, FlushPolicy, turn, turn_scope };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let count = Signal::new(0);
            let _watch = count.sub(|value| print(i"seen {value}"));
            turn(FlushPolicy::AtEnd, || {
                let task = async {
                    await tick();
                    count.set(7);
                };
            });
            print("sync done");
        }
        main();
        "#,
        "seen 0\nsync done\nseen 7\n",
    );
}

#[test]
fn at_suspension_flushes_before_each_await() {
    // The optimistic-paint cadence: writes made BEFORE an await are settled
    // at the suspension point (compiler-inserted, policy-gated), so the
    // first paint happens before the slow work.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, FlushPolicy, turn, turn_scope };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let status = Signal::new("idle");
            let _watch = status.sub(|value| print(i"status {value}"));
            turn(FlushPolicy::AtSuspension, || {
                let task = async {
                    status.set("saving");
                    await tick();
                    status.set("saved");
                };
            });
            print("sync done");
        }
        main();
        "#,
        "status idle\nstatus saving\nsync done\nstatus saved\n",
    );
}

#[test]
fn at_end_holds_writes_across_the_await_inside_the_extent() {
    // The transactional cadence: an AtEnd turn does NOT pre-flush at the
    // suspension — the pre-await write settles with the extent (here, the
    // sync drain at the body's first suspension boundary), not before it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, FlushPolicy, turn, turn_scope };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let status = Signal::new("idle");
            let _watch = status.sub(|value| print(i"status {value}"));
            turn(FlushPolicy::AtEnd, || {
                let task = async {
                    status.set("working");
                    await tick();
                    status.set("done");
                };
                status.set("queued");
            });
            print("sync done");
        }
        main();
        "#,
        "status idle\nstatus queued\nsync done\nstatus done\n",
    );
}

// --- reactive-turns follow-ons: the held turn (an awaiting `turn` body   ---
// --- adapts — the pre-merge `turn_async`) and the optimistic lifecycle.  ---

#[test]
fn an_awaiting_turn_body_holds_writes_until_it_completes() {
    // The transactional extent, through ADAPTATION (the body is a plain
    // closure parameter): NOTHING publishes during the body — not before
    // the await, not in continuations — and the single settle coalesces
    // same-signal writes to the final value ("working" never fires).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, turn, FlushPolicy, turn_scope };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let status = Signal::new("idle");
            let _watch = status.sub(|value| print(i"status {value}"));
            turn(FlushPolicy::AtEnd, || {
                status.set("working");
                tick();
                status.set("done");
            });
            print("after turn");
        }
        main();
        "#,
        "status idle\nstatus done\nafter turn\n",
    );
}

#[test]
fn an_awaiting_turn_returns_the_body_value() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ turn, FlushPolicy, turn_scope };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let answer = turn(FlushPolicy::AtEnd, || {
                tick();
                42
            });
            print(answer);
        }
        main();
        "#,
        "42\n",
    );
}

#[test]
fn a_sync_turn_body_stays_atomic_and_keeps_its_emission() {
    // The other adaptation instance: a synchronous body drains at the end
    // of its synchronous extent — subscribers fire before the next
    // statement runs.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, turn, FlushPolicy, turn_scope };

        fun main() {
            let counter = Signal::new(0);
            let _watch = counter.sub(|value| print(i"saw {value}"));
            turn(FlushPolicy::AtEnd, || {
                counter.set(1);
                counter.set(2);
            });
            print("after");
        }
        main();
        "#,
        "saw 0\nsaw 2\nafter\n",
    );
}

#[test]
fn an_async_void_body_through_a_generic_return_parameter_adapts() {
    // The merge's load-bearing edge: `turn`'s body is `|| T`, and T = void
    // instantiations must ADAPT (await — the sequential contract), not take
    // the declared-void spawn semantics. Spawning here would drain a turn
    // while its body still runs.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::sleep;
        fun run_it<T>(body: || T): T {
            body()
        }
        fun main() {
            run_it(|| {
                sleep(10);
                print("inside");
            });
            print("after");
        }
        "#,
        "inside\nafter\n",
    );
}

#[test]
fn optimistic_paints_then_reconciles_to_the_confirmed_value() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, optimistic };
        import std::result::Result::{ self, Ok, Err };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let label = Signal::new("saved v1");
            let _watch = label.sub(|value| print(i"label {value}"));
            let outcome = optimistic(label, "saving v2", || {
                tick();
                Ok("saved v2")
            });
            match outcome {
                Ok(let value) => print(i"ok {value}"),
                Err(let _e) => print("failed"),
            }
        }
        main();
        "#,
        "label saved v1\nlabel saving v2\nlabel saved v2\nok saved v2\n",
    );
}

#[test]
fn optimistic_rolls_back_on_failure() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, optimistic };
        import std::result::Result::{ self, Ok, Err };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let label = Signal::new("saved v1");
            let _watch = label.sub(|value| print(i"label {value}"));
            let outcome: Result<str, str> = optimistic(label, "saving v2", || {
                tick();
                Err("offline")
            });
            match outcome {
                Ok(let _value) => print("ok"),
                Err(let error) => print(i"failed: {error}"),
            }
        }
        main();
        "#,
        "label saved v1\nlabel saving v2\nlabel saved v1\nfailed: offline\n",
    );
}

// --- A14: the optimistic-write → reconcile lifecycle, made observable and ---
// --- made correct when writes overlap (proposal/optimistic-lifecycle.md). ---
//
// The free `optimistic` above is the stateless one-shot and its two pins are
// the compatibility gate: they must keep their exact output. An
// `Optimistic<T>` cell wraps the same signal and adds what a free function
// over a bare `Signal` has nowhere to keep — a `Pending`/`Rejected` state to
// bind, a generation so only the NEWEST write paints, and a confirmed shadow
// so a rollback lands on server truth rather than on a local value.

/// The `WriteState` renderer every pin below shares.
const OPTIMISTIC_LABEL: &str = r#"
        fun label(state: WriteState): str {
            match state {
                WriteState::Confirmed => "confirmed",
                WriteState::Pending => "pending",
                WriteState::Rejected(let reason) => i"rejected:{reason}",
            }
        }
"#;

#[test]
fn an_optimistic_cell_publishes_pending_then_the_confirmed_value() {
    // The state a spinner binds, and the reconcile to SERVER truth: the
    // commit's `Ok` payload replaces the paint, it is not merely accepted.
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::reactive::{{ Signal, Optimistic, WriteState }};
        import std::result::Result::{{ self, Ok, Err }};
        import std::time::{{ sleep_for, Duration }};
        {OPTIMISTIC_LABEL}
        fun main() {{
            let title = Signal::new("v1");
            let cell = Optimistic::over(title);
            let _watch = cell.state.sub(|state| print(i"state {{label(state)}}"));
            let outcome = cell.write("v2", || {{
                sleep_for(Duration::millis(5));
                let reply: Result<str, str> = Ok("v2-server");
                reply
            }});
            print(i"value {{title.get()}}");
            print(outcome.is_ok());
        }}
        main();
        "#
        ),
        "state confirmed\nstate pending\nstate confirmed\nvalue v2-server\ntrue\n",
    );
}

#[test]
fn an_optimistic_cell_rolls_back_and_publishes_the_rejection() {
    // The failure half: the value returns to the last CONFIRMED truth and
    // the reason lands somewhere a banner can bind, instead of only in the
    // return value the free function hands back.
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::reactive::{{ Signal, Optimistic, WriteState }};
        import std::result::Result::{{ self, Ok, Err }};
        import std::time::{{ sleep_for, Duration }};
        {OPTIMISTIC_LABEL}
        fun main() {{
            let title = Signal::new("v1");
            let cell = Optimistic::over(title);
            let _watch = cell.state.sub(|state| print(i"state {{label(state)}}"));
            let _outcome = cell.write("v2", || {{
                sleep_for(Duration::millis(5));
                let reply: Result<str, str> = Err("offline");
                reply
            }});
            print(i"value {{title.get()}}");
        }}
        main();
        "#
        ),
        "state confirmed\nstate pending\nstate rejected:offline\nvalue v1\n",
    );
}

#[test]
fn a_superseded_optimistic_outcome_does_not_paint_the_cell() {
    // The bug the free function has, fixed by the generation guard. An older
    // write FAILS slowly while a newer one SUCCEEDS quickly: through
    // `optimistic` the stale rollback lands last and the screen shows the
    // cell's original value while the server holds the newer one. The newest
    // write owns the cell, so the older outcome is discarded.
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::reactive::{{ Signal, Optimistic, WriteState }};
        import std::result::Result::{{ self, Ok, Err }};
        import std::time::{{ sleep_for, Duration }};
        {OPTIMISTIC_LABEL}
        fun main() {{
            let title = Signal::new("A");
            let cell = Optimistic::over(title);
            let _older = async cell.write("B", || {{
                sleep_for(Duration::millis(50));
                let reply: Result<str, str> = Err("nope");
                reply
            }});
            let _newer = async cell.write("C", || {{
                sleep_for(Duration::millis(10));
                let reply: Result<str, str> = Ok("C-server");
                reply
            }});
            sleep_for(Duration::millis(90));
            print(i"value {{title.get()}}");
            print(i"state {{label(cell.state.get())}}");
        }}
        main();
        "#
        ),
        "value C-server\nstate confirmed\n",
    );
}

#[test]
fn a_rollback_lands_on_the_last_confirmation_not_the_original_value() {
    // Why the paint guard alone is not enough, and why `confirmed_generation`
    // exists. The OLDER write succeeds (the server really does hold
    // "B-server"); the newer one is refused. The rollback must land on what
    // the server confirmed, not on the value the cell started at — which the
    // paint guard, being about who paints, would never have recorded.
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::reactive::{{ Signal, Optimistic, WriteState }};
        import std::result::Result::{{ self, Ok, Err }};
        import std::time::{{ sleep_for, Duration }};
        {OPTIMISTIC_LABEL}
        fun main() {{
            let title = Signal::new("A");
            let cell = Optimistic::over(title);
            let _older = async cell.write("B", || {{
                sleep_for(Duration::millis(10));
                let reply: Result<str, str> = Ok("B-server");
                reply
            }});
            let _newer = async cell.write("C", || {{
                sleep_for(Duration::millis(50));
                let reply: Result<str, str> = Err("nope");
                reply
            }});
            sleep_for(Duration::millis(90));
            print(i"value {{title.get()}}");
            print(i"state {{label(cell.state.get())}}");
        }}
        main();
        "#
        ),
        "value B-server\nstate rejected:nope\n",
    );
}

#[test]
fn an_out_of_order_confirmation_cannot_walk_the_confirmed_shadow_backwards() {
    // The edge the previous pin does NOT reach: two writes both succeed, and
    // the OLDER one's reply arrives last. Advancing the shadow on arrival
    // order would record "B-server" as server truth after "C-server" already
    // superseded it — so a later rollback would display a value two writes
    // stale. The shadow advances on WRITE order, which is why it carries a
    // generation of its own.
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::reactive::{{ Signal, Optimistic, WriteState }};
        import std::result::Result::{{ self, Ok, Err }};
        import std::time::{{ sleep_for, Duration }};
        {OPTIMISTIC_LABEL}
        fun main() {{
            let title = Signal::new("A");
            let cell = Optimistic::over(title);
            let _older = async cell.write("B", || {{
                sleep_for(Duration::millis(50));
                let reply: Result<str, str> = Ok("B-server");
                reply
            }});
            let _newer = async cell.write("C", || {{
                sleep_for(Duration::millis(10));
                let reply: Result<str, str> = Ok("C-server");
                reply
            }});
            sleep_for(Duration::millis(90));
            print(i"settled {{title.get()}}");
            let _refused = cell.write("D", || {{
                sleep_for(Duration::millis(5));
                let reply: Result<str, str> = Err("nope");
                reply
            }});
            print(i"value {{title.get()}}");
            print(i"state {{label(cell.state.get())}}");
        }}
        main();
        "#
        ),
        "settled C-server\nvalue C-server\nstate rejected:nope\n",
    );
}

#[test]
fn an_optimistic_transition_publishes_one_coherent_wave() {
    // A transition writes TWO signals, so an observer of both must never see
    // half of one — no "new value, still confirmed", no "old value, already
    // pending". `batch` joins the ambient turn when there is one and creates
    // one when there is not; this program has none.
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::reactive::{{ Signal, Optimistic, WriteState, combine }};
        import std::result::Result::{{ self, Ok, Err }};
        import std::time::{{ sleep_for, Duration }};
        {OPTIMISTIC_LABEL}
        fun main() {{
            let title = Signal::new("A");
            let cell = Optimistic::over(title);
            let both = combine((cell.value, cell.state));
            let _watch = both.sub(|pair| {{
                let (value, state) = pair;
                print(i"{{value}}/{{label(state)}}");
            }});
            let _outcome = cell.write("B", || {{
                sleep_for(Duration::millis(5));
                let reply: Result<str, str> = Ok("B-server");
                reply
            }});
        }}
        main();
        "#
        ),
        "A/confirmed\nB/pending\nB-server/confirmed\n",
    );
}

#[test]
fn a_held_turn_holds_the_whole_optimistic_lifecycle() {
    // The transaction wins: inside a `turn` with an awaiting body nothing
    // publishes mid-flight, so the paint and the `Pending` flip are never
    // observed and only the reconciled value reaches subscribers. The cost is
    // real and documented — a "Saving…" indicator inside a held turn never
    // appears.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Optimistic, WriteState, turn, FlushPolicy, turn_scope };
        import std::result::Result::{ self, Ok, Err };

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let title = Signal::new("A");
            let cell = Optimistic::over(title);
            let _watch = title.sub(|value| print(i"held {value}"));
            turn(FlushPolicy::AtEnd, || {
                let _outcome = cell.write("B", || {
                    tick();
                    let reply: Result<str, str> = Ok("B-server");
                    reply
                });
            });
            print("after held turn");
        }
        main();
        "#,
        "held A\nheld B-server\nafter held turn\n",
    );
}

#[test]
fn an_optimistic_cell_reconciles_over_a_real_rpc_round_trip() {
    // The lifecycle against a genuine wire turn rather than a bare `tick()`:
    // `local_rpc` + a `[service]` whose handler suspends, so the commit rides
    // the same `AtEnd` turn a real dispatch does. Both halves in one program —
    // a rename the service accepts, then one it refuses (an empty reply, the
    // shape `walkthrough`'s own commit adapter checks for) — and the call
    // counter proves each write reached the server exactly once.
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::shared::Shared;
        import std::reactive::{{ Signal, Optimistic, WriteState }};
        import std::result::Result::{{ self, Ok, Err }};
        import std::json::{{ Json, json_codec }};
        import std::rpc::{{ local_rpc }};
        import std::time::{{ sleep_for, Duration }};
        {OPTIMISTIC_LABEL}
        [service(TitleClient)]
        struct Titles {{ stored: Shared<str>, calls: Shared<i32> }}

        impl Titles {{
            [rpc]
            fun rename(self, title: str): str {{
                self.calls.write() = self.calls.read() + 1;
                sleep_for(Duration::millis(5));
                if title == "refuse" {{
                    ""
                }} else {{
                    self.stored.write() = i"{{title}}-stored";
                    self.stored.read()
                }}
            }}
        }}

        fun main() {{
            let service = Titles {{ stored = Shared::new("v1"), calls = Shared::new(0) }};
            let transport = local_rpc(service.dispatcher().into_protocol(json_codec()));
            let client = TitleClient {{ transport, codec = json_codec() }};

            let title = Signal::new("v1");
            let cell = Optimistic::over(title);
            let _watch = title.sub(|value| print(i"title {{value}}"));

            let _accepted = cell.write("v2", || {{
                let reply: Result<str, str> = match client.rename("v2") {{
                    Ok(let stored) => if stored == "" {{ Err("save refused") }} else {{ Ok(stored) }},
                    Err(let _error) => Err("rpc error"),
                }};
                reply
            }});
            print(i"state {{label(cell.state.get())}}");

            let _refused = cell.write("refuse", || {{
                let reply: Result<str, str> = match client.rename("refuse") {{
                    Ok(let stored) => if stored == "" {{ Err("save refused") }} else {{ Ok(stored) }},
                    Err(let _error) => Err("rpc error"),
                }};
                reply
            }});
            print(i"state {{label(cell.state.get())}}");
            print(i"calls {{service.calls.read()}}");
        }}
        "#
        ),
        "title v1\ntitle v2\ntitle v2-stored\nstate confirmed\ntitle refuse\ntitle v2-stored\nstate rejected:save refused\ncalls 2\n",
    );
}

// --- backlog J2: `async || T` closure types — asyncness as a type-level ---
// --- contract, so indirect calls await implicitly like direct ones.     ---

#[test]
fn a_call_through_an_async_typed_parameter_awaits() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        async fun tick() {
            let _beat = 1;
        }

        fun run_job(job: async || i32): i32 {
            let value = job();
            print(i"got {value}");
            value
        }

        fun main() {
            let result = run_job(|| {
                tick();
                7
            });
            print(i"result {result}");
        }
        main();
        "#,
        "got 7\nresult 7\n",
    );
}

#[test]
fn a_sync_closure_into_an_async_parameter_is_fine() {
    // The safe direction: awaiting a plain value just resolves.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun run_job(job: async || i32): i32 {
            job()
        }

        fun main() {
            print(run_job(|| 5));
        }
        main();
        "#,
        "5\n",
    );
}

#[test]
fn an_async_closure_into_a_plain_void_parameter_is_spawn_semantics() {
    // Fire-and-forget through a plain `|| void` parameter stays legal — the
    // UI handler / turn-body shape (continuations settle via the turn
    // machinery; no value is lied about).
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        async fun tick() {
            let _beat = 1;
        }

        fun fire(callback: || void) {
            callback();
            print("fired");
        }

        fun main() {
            fire(|| {
                tick();
                print("later");
            });
            print("sync end");
        }
        main();
        "#,
        "fired\nsync end\nlater\n",
    );
}

#[test]
fn an_async_closure_into_a_plain_valued_parameter_adapts() {
    // Once the J2 divergence (the result would be a promise typing as T) —
    // now the adaptation seam (async-polymorphism.md A.1): the async
    // argument instantiates an async `compute`, the call through `producer`
    // awaits, and the caller receives the settled value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        async fun tick() {
            let _beat = 1;
        }

        fun compute(producer: || i32): i32 {
            producer()
        }

        fun main() {
            print(compute(|| {
                tick();
                7
            }));
        }
        "#,
        "7\n",
    );
}

#[test]
fn an_async_closure_type_composes_with_a_context_clause() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let current: Context<i32> = Context::new();

        async fun tick() {
            let _beat = 1;
        }

        fun stage(body: (async || i32) context current): i32 {
            current.run(3, body)
        }

        fun main() {
            let doubled = stage(|| {
                tick();
                current.get() * 2
            });
            print(doubled);
        }
        main();
        "#,
        "6\n",
    );
}

#[test]
fn an_async_annotated_let_awaits_at_its_calls() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        async fun tick() {
            let _beat = 1;
        }

        fun main() {
            let job: async || i32 = || {
                tick();
                11
            };
            print(job());
        }
        main();
        "#,
        "11\n",
    );
}

// --- I4: subscript absence panics (checked subscripts) -----------------------
// `a[i]` — read, write, or `&mut a[i]` view mint — requires `0 <= i < a.len()`;
// a violation panics. Writes never create slots (growth is `push`); `get(i)`
// stays the total `Option` form. The check happens at use / at mint; a deref
// through an already-minted view is the dynamic rule-4 remainder (C2), not
// this item.

#[test]
fn an_out_of_bounds_read_panics() {
    assert_run_panics(
        r#"
        import std::io::print;
        fun main() {
            mut xs: List<i32> = List::new();
            xs.push(10);
            xs.push(20);
            print(xs[5]);
        }
        main();
        "#,
        "index out of bounds: the length is 2 but the index is 5",
    );
}

#[test]
fn an_out_of_bounds_write_panics_rather_than_growing() {
    assert_run_panics(
        r#"
        fun main() {
            mut xs: List<i32> = List::new();
            xs.push(10);
            xs[3] = 9;
        }
        main();
        "#,
        "index out of bounds: the length is 1 but the index is 3",
    );
}

#[test]
fn a_negative_index_panics() {
    assert_run_panics(
        r#"
        import std::io::print;
        fun main() {
            mut xs: List<i32> = List::new();
            xs.push(10);
            let i = 0 - 1;
            print(xs[i]);
        }
        main();
        "#,
        "index out of bounds: the length is 1 but the index is -1",
    );
}

#[test]
fn an_out_of_bounds_view_mint_panics() {
    // The view never comes to exist: the panic fires at `&mut xs[4]`, before
    // `bump` is entered.
    assert_run_panics(
        r#"
        fun bump(slot: &mut i32) {
            slot = *slot + 1;
        }
        fun main() {
            mut xs: List<i32> = List::new();
            xs.push(10);
            bump(&mut xs[4]);
        }
        main();
        "#,
        "index out of bounds: the length is 1 but the index is 4",
    );
}

#[test]
fn an_empty_list_subscript_panics() {
    // view-invalidation.md §1's P1 case: the empty list, subscripted.
    assert_run_panics(
        r#"
        import std::io::print;
        fun main() {
            mut xs: List<i32> = List::new();
            print(xs[0]);
        }
        main();
        "#,
        "index out of bounds: the length is 0 but the index is 0",
    );
}

#[test]
fn in_bounds_subscripts_are_unchanged() {
    // Read, in-place write, and a scalar element view — the subscript.vl
    // shapes, asserted here so the checked emission can't regress them.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun bump(slot: &mut i32) {
            slot = *slot + 100;
        }
        fun main() {
            mut xs: List<i32> = List::new();
            xs.push(10);
            xs.push(20);
            print(xs[0] + xs[1]);
            xs[1] = 99;
            print(xs[1]);
            bump(&mut xs[0]);
            print(xs[0]);
        }
        main();
        "#,
        "30\n99\n110\n",
    );
}

#[test]
fn an_unused_binding_with_an_indexing_initializer_still_panics() {
    // An indexing expression is effectful (it can throw), so dropping the
    // unused binding must not drop the check.
    assert_run_panics(
        r#"
        import std::io::print;
        fun main() {
            mut xs: List<i32> = List::new();
            let _probe = xs[0];
            print("reached");
        }
        main();
        "#,
        "index out of bounds: the length is 0 but the index is 0",
    );
}

#[test]
fn list_get_stays_the_option_form() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut xs: List<i32> = List::new();
            xs.push(10);
            match xs.get(5) {
                Some(let value) => print(value),
                None => print("none"),
            }
            match xs.get(0) {
                Some(let value) => print(value),
                None => print("none"),
            }
        }
        main();
        "#,
        "none\n10\n",
    );
}

#[test]
fn a_macro_time_out_of_bounds_subscript_fails_expansion() {
    // The macro interpreter enforces the same bounds; OOB at expansion time is
    // an expansion failure at the invocation, carrying the panic message.
    assert_fails_spanning(
        r#"
        [probe]
        struct Point {
            x: i32,
        }

        macro fun probe(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            let xs = [1, 2];
            let y = xs[5];
            source("")
        }

        fun main() {}

        main();
        "#,
        "probe",
        "index out of bounds",
    );
}

#[test]
fn an_ungrounded_element_type_gets_a_direct_message() {
    // `mut a = []; a[0]` — the element type never grounds. The old message was
    // circular ("cannot index List (only a `List` is indexable)"); it must say
    // what is actually missing.
    assert_fails_spanning(
        r#"
        fun main() {
            mut a = [];
            let x = a[0];
        }
        main();
        "#,
        "a[0]",
        "element type is never determined",
    );
}

// --- H4: triple-quoted strings ------------------------------------------------
// `"""` ... `"""` is a RAW multi-line string literal: the whitespace before
// the closing delimiter is the indentation prefix stripped from every line,
// the newlines adjoining the delimiters belong to the syntax, and no escape
// processing happens at all (util::trim_multiline_string pins the rules at
// unit level; these pin the pipeline).

#[test]
fn a_triple_quoted_string_trims_to_the_closing_indentation() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let text = """
                    line 1
                line 2

                  line 3
                    
                """;
            print(text);
        }
        main();
        "#,
        "    line 1\nline 2\n\n  line 3\n    \n",
    );
}

#[test]
fn a_triple_quoted_string_is_raw() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let text = """
                escapes \n and \t stay raw, {braces} too
                """;
            print(text);
        }
        main();
        "#,
        "escapes \\n and \\t stay raw, {braces} too\n",
    );
}

#[test]
fn an_empty_triple_quoted_string_is_empty() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let text = """
                """;
            print(text);
            print("after");
        }
        main();
        "#,
        "\nafter\n",
    );
}

#[test]
fn content_after_the_opening_quotes_is_an_error() {
    assert_fails_spanning(
        r#"
        fun main() {
            let x = """oops
                """;
        }
        main();
        "#,
        "oops",
        "nothing may follow the opening",
    );
}

#[test]
fn the_closing_quotes_must_be_alone_on_their_line() {
    assert_fails_spanning(
        r#"
        fun main() {
            let x = """
                alpha
                beta """;
        }
        main();
        "#,
        "                beta ",
        "alone on its line",
    );
}

#[test]
fn insufficient_indentation_is_an_error_naming_the_line() {
    assert_fails_spanning(
        r#"
        fun main() {
            let x = """
                properly_indented
              shallow
                """;
        }
        main();
        "#,
        "              shallow",
        "line 2 of the triple-quoted string is not indented",
    );
}

#[test]
fn a_single_line_triple_quoted_string_is_rejected() {
    // The layout's first rule (ledger row 23's remaining arm): the opening
    // """ must be followed by a newline — a one-line `"""…"""` has no
    // content line to lay out.
    assert_fails_spanning(
        r#"
        fun main() {
            let x = """oops""";
        }
        main();
        "#,
        "oops",
        "must be followed by a newline",
    );
}

#[test]
fn a_macro_emits_source_from_a_triple_quoted_string() {
    // The worlds path: the macro interpreter receives the trimmed VALUE (the
    // transformer trims before emission), so generated source needs no
    // concatenation ceremony for its static skeleton.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun gen(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("""
                fun answer(): i32 {
                    42
                }
                """)
        }

        [gen]
        struct Marker {}

        fun main() {
            print(answer());
        }
        main();
        "#,
        "42\n",
    );
}

// --- H7: interpolated triple-quoted strings -----------------------------------
// `i"""` … `"""` is H4's literal with holes. Two rules, in this order:
//
// 1. TRIMMING FIRST, on the literal's raw text — the same rule and the same code
//    as a plain `"""` (util::multiline_layout), with holes and `\{` / `\}`
//    counting as ordinary characters of that text. So a hole never disturbs its
//    line's indent accounting: the closing delimiter's indentation is stripped
//    from the start of every content line whether that line opens with text, an
//    escape, or a hole.
// 2. FRAGMENTING SECOND, on the trimmed text. Exactly two escapes exist: `\{` and
//    `\}` for a literal brace. Everything else is raw — a backslash before any
//    other character is a literal backslash and that character, with no `\n` /
//    `\t` processing, exactly as in a plain `"""`.

#[test]
fn an_interpolated_triple_quoted_string_trims_and_interpolates() {
    // Holes at line start, mid-line, and adjacent to text; a blank line; a line
    // indented past the prefix keeps its extra indentation.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let who = "world";
            let text = i"""
                hello {who}
                {who} leads

                    indented {who} deeper
                """;
            print(text);
        }
        main();
        "#,
        "hello world\nworld leads\n\n    indented world deeper\n",
    );
}

#[test]
fn an_interpolated_triple_quoted_string_escapes_only_braces() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let x = "X";
            let text = i"""
                literal \{braces\} and a hole {x}
                """;
            print(text);
        }
        main();
        "#,
        "literal {braces} and a hole X\n",
    );
}

#[test]
fn a_backslash_in_an_interpolated_triple_quoted_string_is_literal() {
    // NOTHING else is an escape: `\n` is a backslash and an `n`, `\\` is two
    // backslashes, and a `\` before the end of a line is a backslash — the same
    // near-rawness as the plain form.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let x = "X";
            let text = i"""
                path C:\dir\next {x}
                twice \\ and trailing \
                """;
            print(text);
        }
        main();
        "#,
        "path C:\\dir\\next X\ntwice \\\\ and trailing \\\n",
    );
}

#[test]
fn an_interpolated_triple_quoted_hole_may_hold_a_string_with_braces() {
    // The hole is lexed as code, so a `{` inside a plain string in it is content,
    // not a nested hole.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let x = "X";
            let text = i"""
                {"{not a hole}" + x}
                """;
            print(text);
        }
        main();
        "#,
        "{not a hole}X\n",
    );
}

#[test]
fn an_empty_interpolated_triple_quoted_string_is_empty() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let text = i"""
                """;
            print(text);
            print("after");
        }
        main();
        "#,
        "\nafter\n",
    );
}

#[test]
fn adjacent_quotes_inside_an_interpolated_triple_quoted_string_are_content() {
    // The body is raw and runs to the first `"""`, so `""` and a lone `"` are
    // ordinary characters — including right before a hole.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let x = "X";
            let text = i"""
                say "" and "{x}"
                """;
            print(text);
        }
        main();
        "#,
        "say \"\" and \"X\"\n",
    );
}

#[test]
fn content_after_the_opening_quotes_of_an_interpolated_string_is_an_error() {
    // A malformed shape degrades to its plain twin, so the diagnostic is H4's —
    // spanned on the raw text, which sits one byte further in for the `i` form.
    assert_fails_spanning(
        r#"
        fun main() {
            let x = i"""oops
                """;
        }
        main();
        "#,
        "oops",
        "nothing may follow the opening",
    );
}

#[test]
fn insufficient_indentation_in_an_interpolated_string_names_the_line() {
    assert_fails_spanning(
        r#"
        fun main() {
            let x = i"""
                properly_indented
              shallow
                """;
        }
        main();
        "#,
        "              shallow",
        "line 2 of the triple-quoted string is not indented",
    );
}

#[test]
fn an_unescaped_closing_brace_names_the_escape_that_was_meant() {
    // `\}` is one of the two escapes that exist, which is only meaningful if an
    // unescaped `}` is not already a literal one — and the shape it catches is a
    // hole whose `}` was forgotten. The message states the rule and the
    // sanctioned spelling rather than "found '}' expected a token".
    assert_fails_spanning(
        r#"
        fun main() {
            let x = i"""
                a bare } here
                """;
        }
        main();
        "#,
        "}",
        r"written `\}`",
    );
}

#[test]
fn a_macro_emits_source_from_an_interpolated_triple_quoted_string() {
    // THE payoff: a macro's generated source is a template with holes, written
    // as it will appear — no concatenation ceremony, no `\n` bookkeeping.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun gen(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };
            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [] },
            };
            source(i"""
                fun describe_{target.name}(): str \{
                    "{target.name}"
                \}
                """)
        }

        [gen]
        struct Marker {}

        fun main() {
            print(describe_Marker());
        }
        main();
        "#,
        "Marker\n",
    );
}

// --- H5: the `%` remainder operator -------------------------------------------
// Truncated remainder (the dividend's sign), like Rust and JS agree on. Exact
// for every integer type (unlike `/`, `%` needs no trunc wrap: an integer
// remainder is always representable); BigInt for i53/u53; overloadable through
// `std::operators::Rem` like the arithmetic four.

#[test]
fn remainder_on_i32_follows_the_dividend_sign() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(7 % 3);
            print((0 - 7) % 3);
            print(7 % (0 - 3));
        }
        main();
        "#,
        "1\n-1\n1\n",
    );
}

#[test]
fn remainder_on_floats() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(7.5 % 2f);
        }
        main();
        "#,
        "1.5\n",
    );
}

#[test]
fn remainder_on_i53_is_exact() {
    // i53 is f64-repped (F2 profiled trunc over BigInt); `%` of two in-range
    // integers is exact with no wrap needed.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(9000000000000000i53 % 7i53);
        }
        main();
        "#,
        "5\n",
    );
}

#[test]
fn remainder_on_bigint_values() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(9007199254740993n % 4n);
        }
        main();
        "#,
        "1n\n",
    );
}

#[test]
fn u32_remainder_stays_unsigned() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(4000000000u32 % 7u32);
        }
        main();
        "#,
        "3\n",
    );
}

#[test]
fn remainder_binds_with_product() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(1 + 7 % 3);
            print(2 * 7 % 3);
            print(7 % 3 * 2);
        }
        main();
        "#,
        "2\n2\n2\n",
    );
}

#[test]
fn a_compound_remainder_assignment_works() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut x = 17;
            x %= 5;
            print(x);
        }
        main();
        "#,
        "2\n",
    );
}

#[test]
fn a_user_type_dispatches_through_the_rem_trait() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Rem;

        struct Meters {
            v: i32,
        }

        impl Meters with Rem {
            fun rem(self, b: Self): Self {
                Meters { v = self.v % b.v }
            }
        }

        fun main() {
            let left = Meters { v = 17 };
            let right = Meters { v = 5 };
            print((left % right).v);
        }
        main();
        "#,
        "2\n",
    );
}

// --- B16: methods on generic receivers actually check their arguments ---------
// The hole: `resolve_method_arg_check` reconciled arguments against the RAW
// parameter type — `Type::Generic(T)` reconciles with anything — never applying
// the call's receiver substitution. And an empty `[]` literal erased its
// element (zero-argument `List`), so pushes had no slot to ground. Every case
// below pins one shape of the class.

#[test]
fn an_annotated_lists_push_checks_its_argument() {
    assert_fails_spanning(
        r#"
        fun main() {
            mut a: List<i32> = List::new();
            a.push("text");
        }
        main();
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn a_second_push_conflicting_with_the_first_is_an_error() {
    assert_fails_spanning(
        r#"
        fun main() {
            mut a = List::new();
            a.push(10);
            a.push("text");
        }
        main();
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn an_empty_literal_pushed_two_incompatible_types_is_an_error() {
    // The motivating repro (the former `examples/playground`, pruned in D7).
    assert_fails_spanning(
        r#"
        fun main() {
            mut a = [];
            a.push(10);
            a.push("some text");
        }
        main();
        "#,
        "\"some text\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn an_empty_literals_element_grounds_from_a_push() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut a = [];
            a.push(10);
            print(a[0] + 1);
        }
        main();
        "#,
        "11\n",
    );
}

#[test]
fn a_push_grounds_reads_earlier_in_the_source() {
    // Inference is a fixpoint over the whole function, not a statement walk: a
    // later push types an earlier subscript. (The early read sits behind a
    // length guard — reading before pushing would be a correct I4 panic at
    // runtime; this pins TYPING order-independence.)
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut a = [];
            if a.len() > 0 {
                print(a[0] + 1);
            }
            a.push(10);
            print(a[0] + 1);
        }
        main();
        "#,
        "11\n",
    );
}

#[test]
fn a_generic_structs_method_checks_its_argument() {
    assert_fails_spanning(
        r#"
        struct Holder<T> {
            item: T,
        }

        impl Holder<type T> {
            fun replace(&mut self, value: T): void {
                self.item = value;
            }
        }

        fun main() {
            mut h = Holder { item = 1 };
            h.replace("text");
        }
        main();
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn a_maps_insert_checks_its_value() {
    assert_fails_spanning(
        r#"
        import std::map::Map;
        fun main() {
            mut m: Map<str, i32> = Map::new();
            m.insert("k", "not an int");
        }
        main();
        "#,
        "\"not an int\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn a_never_grounded_list_new_subscript_errors() {
    // Same rule as the empty literal (the I4 diagnostic): reading an element
    // whose type never grounds is an error, not a silent `Unknown`.
    assert_fails_spanning(
        r#"
        fun main() {
            mut a = List::new();
            let first = a[0];
        }
        main();
        "#,
        "a[0]",
        "element type is never determined",
    );
}

#[test]
fn a_never_pushed_lists_len_stays_legal() {
    // The tolerance that must survive: methods that don't touch the element
    // type work on a never-grounded list.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut a = [];
            print(a.len());
        }
        main();
        "#,
        "0\n",
    );
}

#[test]
fn a_for_loop_over_a_grounded_literal_types_its_item() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut a = [];
            a.push(10);
            a.push(20);
            for item in a {
                print(item + 1);
            }
        }
        main();
        "#,
        "11\n21\n",
    );
}

#[test]
fn a_nonempty_literals_push_checks_its_argument() {
    assert_fails_spanning(
        r#"
        fun main() {
            mut a = [1, 2];
            a.push("text");
        }
        main();
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

// --- G2: `const` — compile-time evaluation -------------------------------------
// `const` is a weak-precedence expression prefix: it captures the largest
// expression to its right within the bracket/comma context and evaluates it at
// compile time with the macro interpreter, serializing the plain-data result
// IN PLACE (proposal/const-eval.md). Free variables must be const-known;
// failures are spanned diagnostics; the LSP evaluates explicit consts and
// `vilan check` evaluates as `build` does.

#[test]
fn a_const_expression_folds_to_a_literal() {
    let source = r#"
        import std::io::print;
        fun main() {
            let a = const 1 + 2;
            print(a);
        }
        main();
        "#;
    assert_emits_containing(source, "= 3;");
    assert_compiles_and_runs(source, "3\n");
}

#[test]
fn const_captures_weakly_to_the_expression_end() {
    let source = r#"
        import std::io::print;
        fun main() {
            let a = const 1 + 2 * 3;
            print(a);
        }
        main();
        "#;
    assert_emits_containing(source, "= 7;");
    assert_compiles_and_runs(source, "7\n");
}

#[test]
fn parens_narrow_the_capture() {
    let source = r#"
        import std::io::print;
        fun runtime_part(): i32 {
            5
        }
        fun main() {
            let a = (const 2 * 3) + runtime_part();
            print(a);
        }
        main();
        "#;
    assert_emits_containing(source, "6 + ");
    assert_compiles_and_runs(source, "11\n");
}

#[test]
fn a_const_call_evaluates_through_functions() {
    let source = r#"
        import std::io::print;
        fun square(n: i32): i32 {
            n * n
        }
        fun main() {
            let a = const square(7);
            print(a);
        }
        main();
        "#;
    assert_emits_containing(source, "= 49;");
    assert_compiles_and_runs(source, "49\n");
}

#[test]
fn const_chains_through_const_known_bindings() {
    let source = r#"
        import std::io::print;
        fun main() {
            let x = const 5;
            let y = const x * 2;
            print(y);
        }
        main();
        "#;
    assert_emits_containing(source, "= 10;");
    assert_compiles_and_runs(source, "10\n");
}

#[test]
fn a_literal_initialized_binding_is_const_known() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let x = 5;
            let y = const x + 1;
            print(y);
        }
        main();
        "#,
        "6\n",
    );
}

#[test]
fn a_module_level_const_serves_functions() {
    let source = r#"
        import std::io::print;
        fun doubled(): List<i32> {
            mut result: List<i32> = List::new();
            result.push(2);
            result.push(4);
            result
        }
        let TABLE = const doubled();
        fun main() {
            print(TABLE[0] + TABLE[1]);
        }
        main();
        "#;
    assert_emits_containing(source, "[ 2, 4 ]");
    assert_compiles_and_runs(source, "6\n");
}

#[test]
fn a_const_argument_stops_at_the_comma() {
    let source = r#"
        import std::io::print;
        fun show(a: i32, b: i32) {
            print(a + b);
        }
        fun main() {
            show(const 3 * 4, 1);
        }
        main();
        "#;
    assert_emits_containing(source, "(12,");
    assert_compiles_and_runs(source, "13\n");
}

#[test]
fn a_const_block_runs_statements_at_compile_time() {
    let source = r#"
        import std::io::print;
        fun main() {
            let a = const {
                let left = 2;
                let right = 3;
                left * right
            };
            print(a);
        }
        main();
        "#;
    assert_emits_containing(source, "= 6;");
    assert_compiles_and_runs(source, "6\n");
}

#[test]
fn mut_initialized_by_const_stays_runtime_mutable() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut cache = const 1 + 2;
            cache = cache + 1;
            print(cache);
        }
        main();
        "#,
        "4\n",
    );
}

#[test]
fn a_runtime_parameter_is_rejected_as_a_free_variable() {
    // The diagnostic spans the REFERENCE itself (the last `w` — the first is
    // the declaration).
    let source = r#"
        fun f(w: i32): i32 {
            const w + 1
        }
        fun main() {
            let _x = f(1);
        }
        main();
        "#;
    let reference = source.rfind('w').unwrap();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("runtime value")
                && *range == (reference..reference + 1)),
        "no precise-span diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn a_mut_binding_is_not_const_known() {
    let source = r#"
        fun main() {
            mut q = 5;
            let y = const q + 1;
        }
        main();
        "#;
    let reference = source.rfind('q').unwrap();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("runtime value")
                && *range == (reference..reference + 1)),
        "no precise-span diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn a_call_initialized_binding_is_not_const_known() {
    let source = r#"
        fun mk(): i32 {
            5
        }
        fun main() {
            let z = mk();
            let y = const z + 1;
        }
        main();
        "#;
    let reference = source.rfind('z').unwrap();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("runtime value")
                && *range == (reference..reference + 1)),
        "no precise-span diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn a_panic_at_const_time_is_a_compile_error() {
    // The diagnostic spans the whole const expression. Expression-level spans
    // INSIDE the callee stay the recorded refinement (const-eval.md §8.2 —
    // the interpreted tree carries no positions); the failing FUNCTION is
    // named, which the deep-failure pins below cover.
    let diagnostics = failure_diagnostics(
        r#"
        fun main() {
            let a = const {
                mut xs: List<i32> = List::new();
                xs.push(1);
                xs[5]
            };
        }
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("const evaluation failed")
                && message.contains("index out of bounds")),
        "no const-panic diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn a_capability_is_rejected_at_const_time() {
    assert_fails_spanning(
        r#"
        import std::random::range;
        fun main() {
            let a = const range(1, 6);
        }
        main();
        "#,
        "range(1, 6)",
        "not available",
    );
}

#[test]
fn a_closure_result_is_not_plain_data() {
    assert_fails_spanning(
        r#"
        fun main() {
            let f = const || 1;
        }
        main();
        "#,
        "|| 1",
        "plain data",
    );
}

#[test]
fn the_js_refugee_hint_names_the_idiom() {
    assert_fails_spanning(
        r#"
        fun main() {
            const x = 3;
        }
        main();
        "#,
        "const x = 3",
        "Vilan has no const declarations; write `let x = const ..`",
    );
}

#[test]
fn bigint_and_float_results_serialize_faithfully() {
    let source = r#"
        import std::io::print;
        fun main() {
            let big = const 2n * 3n;
            let precise = const 0.1 + 0.2;
            print(big);
            print(precise);
        }
        main();
        "#;
    assert_emits_containing(source, "6n");
    assert_compiles_and_runs(source, "6n\n0.30000000000000004\n");
}

#[test]
fn struct_and_enum_results_serialize() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        struct Point {
            x: i32,
            y: i32,
        }
        fun main() {
            let p = const Point { x = 1, y = 2 };
            print(p.x + p.y);
            let o = const Some(5);
            match o {
                Some(let value) => print(value),
                None => print("none"),
            }
        }
        main();
        "#,
        "3\n5\n",
    );
}

#[test]
fn a_const_dependency_cycle_is_an_error() {
    assert_fails(
        r#"
        let a: i32 = const b + 1;
        let b: i32 = const a + 1;
        fun main() {}
        main();
        "#,
    );
}

#[test]
fn const_chains_through_computed_bindings() {
    // The dependency is itself a COMPUTED const (not a literal): `y`'s
    // mini-program declares `x` from the stored result, keyed by its
    // initializer expression.
    let source = r#"
        import std::io::print;
        fun square(n: i32): i32 {
            n * n
        }
        fun main() {
            let x = const square(3);
            let y = const x + 1;
            print(y);
        }
        main();
        "#;
    assert_emits_containing(source, "= 10;");
    assert_compiles_and_runs(source, "10\n");
}

// --- G2 slice 5: the asset channel + the const-only bit -----------------------
// `std::asset::emit(kind, line)` accumulates build assets during const
// evaluation (const-eval.md §3); the channel dedups by line and orders
// lexically. `emit` is const-ONLY (§2): a runtime call path errors at the
// boundary call site — the crossing from runtime code into emit-reaching
// territory.

#[test]
fn a_const_emit_collects_assets() {
    let assets = collected_assets(
        r#"
        import std::asset::emit;
        fun rule(): i32 {
            emit("css", ".a{color:red}");
            emit("css", ".b{color:blue}");
            1
        }
        let _style = const rule();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.contains(&("css".to_string(), ".a{color:red}".to_string())),
        "{assets:?}"
    );
    assert!(
        assets.contains(&("css".to_string(), ".b{color:blue}".to_string())),
        "{assets:?}"
    );
}

#[test]
fn assets_deduplicate_and_sort_in_cascade_order() {
    // Two consts emit overlapping lines and a media block; the assembled file
    // dedups and sorts — '.' < '@', so media rules take the LATER cascade
    // position they need (the CSS-soundness argument in assemble_assets).
    let assembled = assembled_assets(
        r#"
        import std::asset::emit;
        fun base(): i32 {
            emit("css", ".pA3{padding:1rem}");
            emit("css", "@media (min-width: 768px){.mX{padding:2rem}}");
            1
        }
        fun accent(): i32 {
            emit("css", ".pA3{padding:1rem}");
            emit("css", ".bC7{background:blue}");
            2
        }
        let _a = const base();
        let _b = const accent();
        fun main() {}
        main();
        "#,
    );
    let css = assembled.get("css").expect("a css asset");
    assert_eq!(
        css,
        ".bC7{background:blue}\n.pA3{padding:1rem}\n@media (min-width: 768px){.mX{padding:2rem}}\n"
    );
}

#[test]
fn media_rules_sort_by_ascending_min_width() {
    // B35: the assembled order must be numeric, not lexical — '1' < '6' put
    // the 1024px rule BEFORE the 640px one, and on a wide viewport (where
    // both medias match and specificity ties) the narrow rule won the
    // cascade. Emission order here is widest-first to prove the sort, not
    // the collection order, decides.
    let assembled = assembled_assets(
        r#"
        import std::asset::emit;
        fun wide(): i32 {
            emit("css", "@media (min-width: 1280px){.d{width:4rem}}");
            emit("css", "@media (min-width: 1024px){.c{width:3rem}}");
            1
        }
        fun narrow(): i32 {
            emit("css", "@media (min-width: 640px){.a{width:1rem}}");
            emit("css", "@media (min-width: 768px){.b{width:2rem}}");
            emit("css", ".base{width:0}");
            2
        }
        let _w = const wide();
        let _n = const narrow();
        fun main() {}
        main();
        "#,
    );
    let css = assembled.get("css").expect("a css asset");
    assert_eq!(
        css,
        ".base{width:0}\n\
         @media (min-width: 640px){.a{width:1rem}}\n\
         @media (min-width: 768px){.b{width:2rem}}\n\
         @media (min-width: 1024px){.c{width:3rem}}\n\
         @media (min-width: 1280px){.d{width:4rem}}\n"
    );
}

#[test]
fn a_sm_lg_pair_renders_the_lg_value_on_a_wide_viewport() {
    // The B35 field case: two breakpoints on the SAME property. The sm rule
    // must precede the lg rule in the assembled stylesheet so the widest
    // matching breakpoint wins the cascade tie.
    let assembled = assembled_assets(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().sm(style().padding(space(2))).lg(style().padding(space(3)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let css = assembled.get("css").expect("a css asset");
    let sm = css
        .find("@media (min-width: 640px)")
        .expect("an sm rule in {css:?}");
    let lg = css
        .find("@media (min-width: 1024px)")
        .expect("an lg rule in {css:?}");
    assert!(
        sm < lg,
        "the sm rule must precede the lg rule so lg wins the wide-viewport cascade tie:\n{css}"
    );
}

#[test]
fn asset_kinds_stay_separate() {
    let assembled = assembled_assets(
        r#"
        import std::asset::emit;
        fun both(): i32 {
            emit("css", ".a{}");
            emit("txt", "hello");
            1
        }
        let _x = const both();
        fun main() {}
        main();
        "#,
    );
    assert_eq!(assembled.get("css").map(String::as_str), Some(".a{}\n"));
    assert_eq!(assembled.get("txt").map(String::as_str), Some("hello\n"));
}

#[test]
fn a_non_css_kind_keeps_lexical_order_for_media_looking_lines() {
    // G5 (build-hooks.md §5.2a, probe P2 inverted into a pin): the cascade
    // comparator is css's alone. A non-css kind holding a line that happens
    // to parse as a media rule must NOT have it forced last — lexical order
    // puts '@' (0x40) before 'z' (0x7A).
    let assembled = assembled_assets(
        r#"
        import std::asset::emit;
        fun entries(): i32 {
            emit("manifest", "zebra: last by bytes");
            emit("manifest", "@media (min-width: 768px){.mX{padding:2rem}}");
            1
        }
        let _e = const entries();
        fun main() {}
        main();
        "#,
    );
    let manifest = assembled.get("manifest").expect("a manifest asset");
    assert_eq!(
        manifest,
        "@media (min-width: 768px){.mX{padding:2rem}}\nzebra: last by bytes\n"
    );
}

#[test]
fn non_css_media_looking_lines_sort_by_bytes_not_by_width() {
    // G5's second half: B35's ascending-min-width override is a CSS cascade
    // property. Between two media-looking lines of a non-css kind the order
    // is lexical — '1' < '6' puts 1024px first — where the css kind would
    // sort 640px first.
    let assembled = assembled_assets(
        r#"
        import std::asset::emit;
        fun entries(): i32 {
            emit("manifest", "@media (min-width: 640px){.a{width:1rem}}");
            emit("manifest", "@media (min-width: 1024px){.c{width:3rem}}");
            1
        }
        let _e = const entries();
        fun main() {}
        main();
        "#,
    );
    let manifest = assembled.get("manifest").expect("a manifest asset");
    assert_eq!(
        manifest,
        "@media (min-width: 1024px){.c{width:3rem}}\n\
         @media (min-width: 640px){.a{width:1rem}}\n"
    );
}

#[test]
fn the_cascade_comparator_stays_css_scoped_in_a_mixed_flush() {
    // One flush, two kinds: css keeps the cascade order (media last,
    // ascending min-width) while the sibling kind sorts the SAME lines
    // lexically. Pins that the rule is per kind, not per flush.
    let assembled = assembled_assets(
        r#"
        import std::asset::emit;
        fun both(): i32 {
            emit("css", "zx{color:red}");
            emit("css", "@media (min-width: 1024px){.c{width:3rem}}");
            emit("css", "@media (min-width: 640px){.a{width:1rem}}");
            emit("manifest", "zx{color:red}");
            emit("manifest", "@media (min-width: 1024px){.c{width:3rem}}");
            emit("manifest", "@media (min-width: 640px){.a{width:1rem}}");
            1
        }
        let _b = const both();
        fun main() {}
        main();
        "#,
    );
    assert_eq!(
        assembled.get("css").expect("a css asset"),
        "zx{color:red}\n\
         @media (min-width: 640px){.a{width:1rem}}\n\
         @media (min-width: 1024px){.c{width:3rem}}\n"
    );
    assert_eq!(
        assembled.get("manifest").expect("a manifest asset"),
        "@media (min-width: 1024px){.c{width:3rem}}\n\
         @media (min-width: 640px){.a{width:1rem}}\n\
         zx{color:red}\n"
    );
}

#[test]
fn a_runtime_emit_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::asset::emit;
        fun main() {
            emit("css", ".a{}");
        }
        main();
        "#,
        r#"emit("css", ".a{}")"#,
        "compile-time-only",
    );
}

#[test]
fn a_runtime_call_reaching_emit_is_rejected_at_the_boundary() {
    // The error sits at main's CALL into emit-reaching territory — the
    // outermost runtime crossing — not at the emit inside `rule`. (rfind:
    // the declaration `fun rule():` also contains the snippet.)
    let source = r#"
        import std::asset::emit;
        fun rule(): i32 {
            emit("css", ".a{}");
            1
        }
        fun main() {
            let _x = rule();
        }
        main();
        "#;
    let call = source.rfind("rule()").unwrap();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("compile-time-only")
                && *range == (call..call + "rule()".len())),
        "no boundary diagnostic at the call: {diagnostics:#?}"
    );
}

#[test]
fn a_top_level_runtime_call_reaching_emit_is_rejected() {
    let source = r#"
        import std::asset::emit;
        fun rule(): i32 {
            emit("css", ".a{}");
            1
        }
        let _style = rule();
        fun main() {}
        main();
        "#;
    let call = source.rfind("rule()").unwrap();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("compile-time-only")
                && *range == (call..call + "rule()".len())),
        "no top-level boundary diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn reaching_functions_inside_const_are_fine() {
    // The styling shape: property functions bottom out in emit, called from
    // const chains — legal, and the assets flow.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::asset::emit;
        fun padding(): i32 {
            emit("css", ".pA3{padding:1rem}");
            4
        }
        fun main() {
            let width = const padding() * 2;
            print(width);
        }
        main();
        "#,
        "8\n",
    );
}

#[test]
fn analysis_leaves_the_const_results_on_the_program() {
    // The invariant the LSP relies on (const-eval.md §8.3): `analyze_source`
    // already evaluated every `const`, so no consumer needs a second pass to
    // read the values — hover reads `program.const_results` directly.
    let source = r#"
        fun square(n: i32): i32 { n * n }
        fun main() {
            let _folded = const square(7);
        }
        main();
        "#
    .to_string();
    let values = std::thread::Builder::new()
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
            assert!(errors.is_empty(), "expected a clean analysis: {errors:#?}");
            program
                .map(|program| program.const_results.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(
        values,
        vec![vilan_core::interpreter::ConstValue::Number(49.0)],
        "the folded value must survive on the program"
    );
}

// Deep failure attribution (const-eval.md §8.2): the primary span stays the
// `const` expression — there is no inner span to move to — but the frame trace
// names the function the failure happened in and notes its declaration.

#[test]
fn a_deep_const_failure_names_the_failing_function() {
    assert_fails_noting(
        r#"
        fun level_three(xs: List<i32>): i32 {
            xs[9]
        }
        fun level_two(xs: List<i32>): i32 {
            level_three(xs) + 1
        }
        fun level_one(): i32 {
            mut xs: List<i32> = List::new();
            xs.push(1);
            level_two(xs)
        }
        fun main() {
            let _value = const level_one();
        }
        main();
        "#,
        "const evaluation failed in `level_three`: index out of bounds",
        "level_three",
        "the compile-time call chain: level_one → level_two → level_three",
    );
}

#[test]
fn a_single_frame_const_failure_notes_the_declaration_without_a_chain() {
    assert_fails_noting(
        r#"
        import std::io::panic;
        fun only(): i32 {
            panic("no");
            1
        }
        fun main() {
            let _value = const only();
        }
        main();
        "#,
        "const evaluation failed in `only`: no",
        "only",
        "`only` is declared here",
    );
}

#[test]
fn a_const_fuel_miss_reports_a_budget_not_a_failure() {
    // §4's promised wording, which the raw interpreter message never carried.
    assert_fails_with(
        r#"
        fun spin(): i32 {
            mut i = 0;
            for {
                i = i + 1;
            }
            i
        }
        fun main() {
            let _value = const spin();
        }
        main();
        "#,
        "const evaluation did not finish within the compile-time budget in `spin`: the fuel \
         budget was exhausted",
    );
}

#[test]
fn a_const_depth_miss_reports_a_budget_and_elides_the_repeated_frames() {
    assert_fails_noting(
        r#"
        fun recurse(n: i32): i32 {
            recurse(n + 1)
        }
        fun main() {
            let _value = const recurse(0);
        }
        main();
        "#,
        "const evaluation did not finish within the compile-time budget in `recurse`: the \
         call-depth cap was exceeded",
        "recurse",
        "the compile-time call chain: … → recurse → recurse → recurse → recurse",
    );
}

// The value escape (const-eval.md §2): a call THROUGH a function or closure
// value resolves to `Indirect(Value)`, which carries no caller edge, so the
// R-fixpoint cannot follow it. v1 refuses at the reference — without which the
// emitted JS carries a live `__emit_asset` call that has no runtime binding.

#[test]
fn a_function_reaching_emit_cannot_escape_as_a_value() {
    let source = r#"
        import std::asset::emit;
        fun styled(): i32 {
            emit("css", ".a{}");
            1
        }
        fun apply(f: || i32): i32 {
            f()
        }
        fun main() {
            let _x = apply(styled);
        }
        main();
        "#;
    let reference = source.rfind("styled").unwrap();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("compile-time-only")
                && *range == (reference..reference + "styled".len())),
        "no value-escape diagnostic at the reference: {diagnostics:#?}"
    );
}

#[test]
fn a_module_level_value_reference_to_an_emit_reaching_function_is_rejected() {
    let source = r#"
        import std::asset::emit;
        fun styled(): i32 {
            emit("css", ".a{}");
            1
        }
        let HANDLER = styled;
        fun main() {}
        main();
        "#;
    let reference = source.rfind("styled").unwrap();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("compile-time-only")
                && *range == (reference..reference + "styled".len())),
        "no value-escape diagnostic at the module-level reference: {diagnostics:#?}"
    );
}

#[test]
fn a_closure_reaching_emit_cannot_escape_as_a_value() {
    assert_fails_with(
        r#"
        import std::asset::emit;
        fun apply(f: || i32): i32 {
            f()
        }
        fun main() {
            let _x = apply(|| {
                emit("css", ".a{}");
                1
            });
        }
        main();
        "#,
        "compile-time-only",
    );
}

#[test]
fn a_closure_wrapping_an_emit_reaching_call_cannot_escape_as_a_value() {
    assert_fails_with(
        r#"
        import std::asset::emit;
        fun styled(): i32 {
            emit("css", ".a{}");
            1
        }
        fun apply(f: || i32): i32 {
            f()
        }
        fun main() {
            let _x = apply(|| styled());
        }
        main();
        "#,
        "compile-time-only",
    );
}

#[test]
fn an_indirect_call_rooted_in_const_stays_legal() {
    // The refusal is about RUNTIME escape only: inside a `const` expression the
    // interpreter calls through the value happily, and the asset still flows.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::asset::emit;
        fun styled(): i32 {
            emit("css", ".a{}");
            1
        }
        fun apply(f: || i32): i32 {
            f()
        }
        fun main() {
            print(const apply(styled));
        }
        main();
        "#,
        "1\n",
    );
    let assets = collected_assets(
        r#"
        import std::io::print;
        import std::asset::emit;
        fun styled(): i32 {
            emit("css", ".a{}");
            1
        }
        fun apply(f: || i32): i32 {
            f()
        }
        fun main() {
            print(const apply(styled));
        }
        main();
        "#,
    );
    assert_eq!(assets, vec![("css".to_string(), ".a{}".to_string())]);
}

// --- K13 step 2: the const input channel — `asset::read` -----------------------
// The channel's input direction (docs-port.md §3.3, markdown.md §7): const
// code reads a project file at build time. Paths resolve against the package
// root; the file becomes a tracked build input on `program.const_input_files`;
// a miss is a clean diagnostic at the read site; and `read` is const-only
// under exactly the machinery that colors `emit`.

/// A clean analysis with an EXPLICIT package root — the `asset::read` pins
/// point it at a fixture directory (or at the real book) — returning the
/// folded const values and the recorded read inputs.
fn const_reads(
    source: &str,
    root: &Path,
) -> (
    Vec<vilan_core::interpreter::ConstValue>,
    Vec<(PathBuf, Option<u64>)>,
) {
    let source = source.to_string();
    let root = root.to_path_buf();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &root,
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(errors.is_empty(), "expected a clean analysis: {errors:#?}");
            let program = program.expect("a clean analysis leaves a program");
            (
                program.const_results.values().cloned().collect::<Vec<_>>(),
                program.const_input_files.clone(),
            )
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn a_const_read_parses_the_books_largest_page_within_budget() {
    // THE step-2 workload (docs-port.md §2.1 located the wall; this is the
    // wall coming down): const code reads the book's REAL largest page —
    // `docs/spec/memory.md`, the golden fixtures' heaviest — and runs
    // `std::markdown::parse` over it inside const evaluation, within the
    // measured-and-raised fuel budget (2,001,457 fuel measured at the raise;
    // this pin goes red if the page or the parser ever outgrows the budget,
    // which is exactly the honest signal to re-measure). The read lands on
    // `const_input_files` with a content hash: the file is a tracked build
    // input, not an untracked ambient dependency.
    let book_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan");
    let (values, inputs) = const_reads(
        r#"
        import std::asset;
        import std::markdown;
        import std::result::Result::{ Err, Ok };
        fun block_count(): i32 {
            match markdown::parse(asset::read("docs/spec/memory.md")) {
                Ok(let doc) => doc.blocks.len()
                Err(let error) => 0 - 1
            }
        }
        fun main() {
            let _blocks = const block_count();
        }
        main();
        "#,
        &book_root,
    );
    assert_eq!(values.len(), 1, "one folded const: {values:?}");
    assert!(
        matches!(values[0], vilan_core::interpreter::ConstValue::Number(n) if n > 0.0),
        "the page must parse to a positive block count at compile time: {values:?}"
    );
    assert_eq!(inputs.len(), 1, "one tracked input: {inputs:?}");
    assert!(
        inputs[0].0.ends_with("docs/spec/memory.md") && inputs[0].1.is_some(),
        "the read must be recorded as a hashed build input: {inputs:?}"
    );
}

#[test]
fn a_missing_read_is_a_clean_diagnostic_at_the_read_site() {
    assert_fails_spanning(
        r#"
        import std::asset;
        fun main() {
            let _text = const asset::read("vilan-k13-definitely-missing.md");
        }
        main();
        "#,
        r#"asset::read("vilan-k13-definitely-missing.md")"#,
        "cannot read `vilan-k13-definitely-missing.md`",
    );
}

#[test]
fn an_absolute_read_path_is_refused() {
    // Refused lexically, before any filesystem look: the channel reads THE
    // PROJECT, so the build can track every input it depends on.
    //
    // The path is per-platform because `is_absolute` is (N26): a POSIX
    // `/etc/hostname` is NOT absolute to Windows, which wants a drive prefix,
    // so hardcoding it tested the sibling `leaves it` arm there and asserted
    // this arm's wording against it. Both arms refuse on both platforms — the
    // POSIX spelling reaches Windows' escape check as a `RootDir` component —
    // so what was wrong was the pin, not the fence. This one names THIS arm on
    // each platform, and `a_posix_absolute_path_is_refused_on_every_platform`
    // below holds the property that does not vary.
    let absolute = if cfg!(windows) {
        r"C:\Windows\system.ini"
    } else {
        "/etc/hostname"
    };
    assert_fails_with(
        &format!(
            r#"
        import std::asset;
        fun main() {{
            let _text = const asset::read("{}");
        }}
        main();
        "#,
            absolute.replace('\\', "\\\\")
        ),
        &format!("`asset::read` paths are relative to the package root; `{absolute}` is absolute"),
    );
}

#[test]
fn a_posix_absolute_path_is_refused_on_every_platform() {
    // The invariant the platform-split pin above must not lose: a POSIX
    // absolute path never reads the host filesystem, whatever the host thinks
    // "absolute" means. On Linux the first arm refuses it; on Windows the
    // component scan does, because `/` parses as a `RootDir` that is neither
    // `Normal` nor `CurDir`. Asserting the shared half of both messages keeps
    // this true on both without asserting either arm's wording.
    assert_fails_with(
        r#"
        import std::asset;
        fun main() {
            let _text = const asset::read("/etc/hostname");
        }
        main();
        "#,
        "package root",
    );
}

#[test]
fn a_read_path_escaping_the_package_root_is_refused() {
    assert_fails_with(
        r#"
        import std::asset;
        fun main() {
            let _text = const asset::read("../outside.md");
        }
        main();
        "#,
        "`asset::read` paths stay inside the package root as written; `../outside.md` leaves it",
    );
}

// --- E94: `asset::emit`'s kind is an output-path segment, and must be one ---
//
// The read fence above is lexical and TOTAL: `asset::read` refuses an absolute
// path, and refuses any component that is not `Normal`/`CurDir`, before any
// filesystem look. `asset::emit`'s kind gets none of that, and it is the same
// kind of thing — the kind becomes a filename beside the build output
// (`write_assets`: `output_js.with_extension(kind)`), so a kind carrying `..`
// or a separator directs a write out of `dist/` entirely. One rule covers both
// shapes, because both are the same mistake: a kind names ONE file, so it must
// be a single path segment. Backlog E94.

/// The wording the fix should use, mirroring the read fence's shape ("`…` paths
/// stay inside the package root as written; `…` leaves it") for the write direction.
const EMIT_KIND_REFUSAL: &str = "`asset::emit` kinds name one file beside the build output";

#[test]
fn an_emit_kind_escaping_the_output_directory_is_refused() {
    assert_fails_with(
        r#"
        import std::asset::emit;
        fun rule(): i32 {
            emit("../evil", "x");
            1
        }
        let _asset = const rule();
        fun main() {}
        main();
        "#,
        EMIT_KIND_REFUSAL,
    );
}

#[test]
fn an_emit_kind_carrying_a_separator_is_refused() {
    // Not an escape, but the same mistake: `a/b` is two segments, so it names a
    // file in a directory the build never made. Refused by the same rule.
    assert_fails_with(
        r#"
        import std::asset::emit;
        fun rule(): i32 {
            emit("a/b", "x");
            1
        }
        let _asset = const rule();
        fun main() {}
        main();
        "#,
        EMIT_KIND_REFUSAL,
    );
}

#[test]
fn a_legitimate_emit_kind_is_untouched() {
    // The green negative: the fence must refuse the two shapes above and
    // NOTHING else — an ordinary kind is what every real `emit` passes, and a
    // rule that refused it would be worse than the hole it closes.
    assert_compiles(
        r#"
        import std::asset::emit;
        fun rule(): i32 {
            emit("css", ".a{color:red}");
            1
        }
        let _asset = const rule();
        fun main() {}
        main();
        "#,
    );
}

// --- G7: a kind may not name a file the build's own namespace owns ---
//
// E94 above fenced the kind's SHAPE and stopped there, and shape was only half
// the question: one path segment beside the build output can still be a segment
// the build writes there itself. The probe that found it is the first pin below
// — `emit("vl", "CLOBBERED")` in a bare build OVERWROTE THE ENTRY SOURCE FILE
// and exited 0, because a lone package's outputs sit exactly where its entry
// does. `mjs`/`js` take the compiled bundle, `chunks.json` the build manifest,
// and `<arm>.js` the route-chunk namespace (that one worse than an overwrite:
// `sweep_stale_chunks` DELETES what lands there on the next build).
//
// The refusal reads ONE list — `const_eval::build_owned_emit_kind` — which is
// the same list the CLI's per-kind prune reads (`recordable_emit_kind`, G6), so
// the write side and the prune side cannot come to disagree about what the
// build owns. `css` is the one member the fence admits: the build owns it AND
// `emit` is how it is written. Backlog G7, build-hooks.md §5.6.

/// The wording of the namespace half of the fence, as against E94's shape half
/// (`EMIT_KIND_REFUSAL` above): the shared lead-in plus the per-kind clause
/// naming WHICH of the build's files the kind would have taken. Pinning both
/// halves keeps a refusal from drifting into "reserved" with no reason in it.
fn owned_refusal(kind: &str, collides_with: &str) -> String {
    format!(
        "`asset::emit` kinds name one file beside the build output, and \
         `{kind}` collides with {collides_with}"
    )
}

/// A program whose `const` initializer emits one line under `kind`.
fn emitting_program(kind: &str) -> String {
    format!(
        r#"
        import std::asset::emit;
        fun rule(): i32 {{
            emit("{kind}", "x");
            1
        }}
        let _asset = const rule();
        fun main() {{}}
        main();
        "#
    )
}

#[test]
fn an_emit_kind_naming_the_entry_source_is_refused() {
    // The probe inverted: this exact program overwrote its own source with
    // `x` and exited 0 before the fence existed.
    assert_fails_with(
        &emitting_program("vl"),
        &owned_refusal("vl", "the entry source a build's outputs sit beside"),
    );
}

#[test]
fn an_emit_kind_naming_the_process_bundle_is_refused() {
    assert_fails_with(
        &emitting_program("mjs"),
        &owned_refusal("mjs", "the compiled bundle"),
    );
}

#[test]
fn an_emit_kind_naming_the_browser_bundle_is_refused() {
    // The same file on the other platform — one member of the list, two
    // spellings, because a leg's bundle extension follows its target.
    assert_fails_with(
        &emitting_program("js"),
        &owned_refusal("js", "the compiled bundle"),
    );
}

#[test]
fn an_emit_kind_naming_the_build_manifest_is_refused() {
    assert_fails_with(
        &emitting_program("chunks.json"),
        &owned_refusal("chunks.json", "the build manifest"),
    );
}

#[test]
fn an_emit_kind_in_the_route_chunk_namespace_is_refused() {
    // A pattern, not a name: every `<arm>.js` belongs to the chunk sweep, so
    // the fence refuses the family rather than an enumeration of arms.
    assert_fails_with(
        &emitting_program("Route_Docs.js"),
        &owned_refusal("Route_Docs.js", "the build's route-chunk namespace"),
    );
}

#[test]
fn a_computed_emit_kind_is_fenced_like_a_literal_one() {
    // The fence is taken on the const-evaluated VALUE, not on the syntax of
    // the argument, so a kind assembled at const time is refused exactly as a
    // literal is — and it clobbered the source exactly as the literal did
    // before the fix. There is no third path: `asset::emit` is const-only, so
    // the kind is always known where the fence runs.
    assert_fails_with(
        r#"
        import std::asset::emit;
        fun kind_name(): str {
            "v" + "l"
        }
        fun rule(): i32 {
            emit(kind_name(), "x");
            1
        }
        let _asset = const rule();
        fun main() {}
        main();
        "#,
        &owned_refusal("vl", "the entry source a build's outputs sit beside"),
    );
}

#[test]
fn the_style_sidecar_kind_stays_admitted() {
    // `css` IS in the owned list — the prune may never touch `<leg>.css`,
    // because `sweep_stale_sidecar` owns it — and it is nonetheless the one
    // member `emit` admits, because `emit("css", …)` is how the styling system
    // writes that very file. A fence refusing it would take the whole styling
    // system with it, which is why the exemption is a named variant rather
    // than a second copy of the list with one name missing.
    assert_compiles(&emitting_program("css"));
}

#[test]
fn an_emit_kind_the_build_does_not_own_stays_admitted() {
    // The green negative for the namespace half, as
    // `a_legitimate_emit_kind_is_untouched` is for the shape half: a kind of
    // the program's own is exactly what the refusal tells the user to pass, so
    // it had better still compile.
    assert_compiles(&emitting_program("routes"));
}

// --- build-hooks S3: `emit_keyed` — the contribution carries its key ---------
//
// `asset::emit_keyed(kind, key, line)` is `emit`'s ORDERED spelling
// (build-hooks.md §5.3; §10 Q3 ruled 2026-08-28). The flush orders a kind's
// contributions by `(key, line)` and deduplicates by that same pair. The key is
// data the CONTRIBUTOR computes — a route's path, an icon's name, a zero-padded
// rank — and the flush neither writes it nor re-derives it from the line, which
// is the whole reason it is a parameter rather than a comparator: a flush-side
// rule would have to parse the line back, the invented second source of truth
// this tree refuses on principle (§11's rejected alternative).
//
// The slice rests on ONE identity: `emit(kind, line)` IS
// `emit_keyed(kind, line, line)`. G5 chose the non-css lexical order precisely
// so it would hold, and the interpreter records both spellings through one arm
// — one fence call, one push — so it holds by construction rather than by
// inspection. `an_unkeyed_emit_records_the_line_as_its_own_key` pins the
// recording end of it and
// `the_two_spellings_of_one_line_set_assemble_to_the_same_bytes` the flush end;
// between them, an existing kind's bytes have no path along which to move.

/// A program whose `const` initializer makes each `(key, line)` contribution
/// under `kind`, through the keyed spelling.
fn keyed_program(kind: &str, contributions: &[(&str, &str)]) -> String {
    let calls = contributions
        .iter()
        .map(|(key, line)| format!("            emit_keyed(\"{kind}\", \"{key}\", \"{line}\");\n"))
        .collect::<String>();
    format!(
        r#"
        import std::asset::emit_keyed;
        fun contribute(): i32 {{
{calls}            1
        }}
        let _c = const contribute();
        fun main() {{}}
        main();
        "#
    )
}

#[test]
fn a_key_orders_a_kind_against_its_lines_own_order() {
    // THE case plain `emit` cannot express, and the reason the parameter
    // exists: the keys order the lines the opposite way their bytes would.
    // `emit` on these two lines can only ever produce `aaa` then `zzz`.
    let assembled = assembled_assets(&keyed_program(
        "routes",
        &[("0", "zzz-emitted-first"), ("1", "aaa-emitted-second")],
    ));
    assert_eq!(
        assembled.get("routes").map(String::as_str),
        Some("zzz-emitted-first\naaa-emitted-second\n")
    );
}

#[test]
fn one_key_over_several_lines_falls_back_to_the_line() {
    // The secondary sort. A key shared by several contributions still orders
    // them by CONTENT — never by the order the calls happened — so §5.1's rule
    // (the bytes are a function of the set) survives a coarse key. Emitted
    // last-first to prove the sort decides.
    let assembled = assembled_assets(&keyed_program(
        "routes",
        &[("shared", "second"), ("shared", "first")],
    ));
    assert_eq!(
        assembled.get("routes").map(String::as_str),
        Some("first\nsecond\n")
    );
}

#[test]
fn one_line_under_two_keys_survives_twice() {
    // Dedup is per PAIR, not per line (§5.3: the output is a function of the
    // set of `(key, line)` pairs). Two contributions of one line under two keys
    // are two contributions — a ranked list repeating an entry is the shape —
    // and the file holds it once per key.
    let assembled = assembled_assets(&keyed_program(
        "routes",
        &[("a", "same-line"), ("b", "same-line")],
    ));
    assert_eq!(
        assembled.get("routes").map(String::as_str),
        Some("same-line\nsame-line\n")
    );
}

#[test]
fn identical_keyed_contributions_deduplicate() {
    // The other half of dedup-per-pair, and the property that lets independent
    // const code compose: the same contribution made twice is one line, exactly
    // as an un-keyed `emit` of one line twice has always been.
    let assembled = assembled_assets(&keyed_program(
        "routes",
        &[("a", "once"), ("a", "once"), ("a", "once")],
    ));
    assert_eq!(assembled.get("routes").map(String::as_str), Some("once\n"));
}

#[test]
fn a_key_computed_at_const_time_orders_the_flush() {
    // The key is const-time DATA, not a literal — which is the point of §5.3's
    // "the contributor is the only code that knows it". A zero-padded rank is
    // the paper's own example, and it is computed here rather than written.
    let assembled = assembled_assets(
        r#"
        import std::asset::emit_keyed;
        fun rank(index: i32): str {
            if index < 10 { i"0{index}" } else { i"{index}" }
        }
        fun contribute(): i32 {
            emit_keyed("routes", rank(9), "ninth");
            emit_keyed("routes", rank(10), "tenth");
            emit_keyed("routes", rank(2), "second");
            1
        }
        let _c = const contribute();
        fun main() {}
        main();
        "#,
    );
    // Unpadded, "10" would sort before "2" and before "9" — the padding is what
    // the contributor computes to make a lexical order a numeric one.
    assert_eq!(
        assembled.get("routes").map(String::as_str),
        Some("second\nninth\ntenth\n")
    );
}

#[test]
fn a_key_never_reaches_the_file() {
    // Only the LINE is written. A key leaking into the output would be its own
    // defect class — an accumulator whose file depends on a value the author
    // chose for ordering alone.
    let assembled = assembled_assets(&keyed_program("routes", &[("KEY-SENTINEL", "the line")]));
    assert_eq!(
        assembled.get("routes").map(String::as_str),
        Some("the line\n")
    );
}

#[test]
fn an_unkeyed_emit_records_the_line_as_its_own_key() {
    // The desugar, at the recording end: `emit(kind, line)` IS
    // `emit_keyed(kind, line, line)` (§5.3). This is what makes every shipped
    // kind's bytes identical BY CONSTRUCTION — ordering and deduplicating by
    // `(line, line)` is ordering and deduplicating by the line — so it is
    // pinned on the contribution rather than inferred from the file.
    let contributions = collected_keyed_assets(
        r#"
        import std::asset::emit;
        fun contribute(): i32 {
            emit("routes", "a line");
            1
        }
        let _c = const contribute();
        fun main() {}
        main();
        "#,
    );
    assert_eq!(
        contributions,
        vec![vilan_core::const_eval::EmittedAsset {
            kind: "routes".to_string(),
            key: "a line".to_string(),
            line: "a line".to_string(),
        }]
    );
}

#[test]
fn the_two_spellings_of_one_line_set_assemble_to_the_same_bytes() {
    // The slice's gate, as a differential rather than as a golden: the same
    // lines contributed through `emit(kind, line)` and through
    // `emit_keyed(kind, line, line)` produce byte-identical files. The corpus
    // and the assets e2e hold the same property over every shipped kind; this
    // holds the REASON, so a future change that moved the un-keyed spelling off
    // the identity reddens here first and with the shortest explanation.
    let lines = [
        "zebra",
        "@media (min-width: 1024px){.c{}}",
        "apple",
        "@media (min-width: 640px){.a{}}",
        "zebra",
    ];
    let unkeyed = lines
        .iter()
        .map(|line| format!("            emit(\"routes\", \"{line}\");\n"))
        .collect::<String>();
    let keyed = lines
        .iter()
        .map(|line| format!("            emit_keyed(\"routes\", \"{line}\", \"{line}\");\n"))
        .collect::<String>();
    let program = |import: &str, calls: &str| {
        format!(
            r#"
        import std::asset::{import};
        fun contribute(): i32 {{
{calls}            1
        }}
        let _c = const contribute();
        fun main() {{}}
        main();
        "#
        )
    };
    let through_emit = assembled_assets(&program("emit", &unkeyed));
    let through_emit_keyed = assembled_assets(&program("emit_keyed", &keyed));
    assert_eq!(
        through_emit, through_emit_keyed,
        "the two spellings of one line set must produce identical files"
    );
    // …and not vacuously: the file is the one lexical order the un-keyed
    // spelling has always produced, dedup included.
    assert_eq!(
        through_emit.get("routes").map(String::as_str),
        Some(
            "@media (min-width: 1024px){.c{}}\n\
             @media (min-width: 640px){.a{}}\n\
             apple\n\
             zebra\n"
        )
    );
}

#[test]
fn the_two_spellings_interleave_within_one_kind() {
    // A kind is not owned by one spelling. Because an un-keyed contribution
    // keys itself by its line, the two sort into ONE `(key, line)` order — the
    // keyed `"0"` lands ahead of an un-keyed line beginning `a`, and the keyed
    // `"z…"` behind it, with no rule of its own for the mixture.
    let assembled = assembled_assets(
        r#"
        import std::asset::{ emit, emit_keyed };
        fun contribute(): i32 {
            emit("routes", "apple");
            emit_keyed("routes", "0", "keyed-first");
            emit_keyed("routes", "zzz", "keyed-last");
            1
        }
        let _c = const contribute();
        fun main() {}
        main();
        "#,
    );
    assert_eq!(
        assembled.get("routes").map(String::as_str),
        Some("keyed-first\napple\nkeyed-last\n")
    );
}

#[test]
fn emit_keyed_refuses_the_style_sidecar() {
    // `css` is the one owned kind `emit` ADMITS (G7) and the one kind
    // `emit_keyed` refuses — because the sheet's order is the CASCADE's, and
    // `assemble_assets`'s comparator reads the line and never the key. A key
    // accepted here would be silently dropped, which is a wrong answer where a
    // refusal costs nothing; refusing is also what keeps §5.3's noted migration
    // open, since a key that had been ignored could not be given a meaning
    // later.
    assert_fails_with(
        &keyed_program("css", &[("band", ".a{color:red}")]),
        "`asset::emit_keyed` cannot order the `css` kind: the style sidecar is \
         ordered by the CSS cascade, not by a contribution's key",
    );
}

#[test]
fn emit_keyed_inherits_the_owned_kind_refusal() {
    // ONE list, now three consumers (build_owned_emit_kind: the prune, `emit`,
    // and this) — the keyed spelling is not a second door into the build's own
    // output namespace. Every member, because an enumeration is what a shared
    // list is worth: a fence that let `emit_keyed("vl", …)` through would
    // overwrite the entry source exactly as G7's defect did.
    for (kind, collides_with) in [
        ("vl", "the entry source a build's outputs sit beside"),
        ("mjs", "the compiled bundle"),
        ("js", "the compiled bundle"),
        ("chunks.json", "the build manifest"),
        ("Route_Docs.js", "the build's route-chunk namespace"),
    ] {
        assert_fails_with(
            &keyed_program(kind, &[("k", "x")]),
            &format!(
                "`asset::emit_keyed` kinds name one file beside the build output, \
                 and `{kind}` collides with {collides_with}"
            ),
        );
    }
}

#[test]
fn an_emit_keyed_kind_carrying_a_separator_is_refused() {
    // E94's shape half, named for the spelling that tripped it. The kind is one
    // file beside the build output whichever function put it there.
    assert_fails_with(
        &keyed_program("a/b", &[("k", "x")]),
        "`asset::emit_keyed` kinds name one file beside the build output",
    );
}

#[test]
fn an_emit_keyed_kind_the_build_does_not_own_stays_admitted() {
    // The green negative: the two refusals above must take those kinds and
    // nothing else.
    assert_compiles(&keyed_program("routes", &[("k", "x")]));
}

#[test]
fn a_runtime_emit_keyed_is_rejected() {
    // The const-only fence, the same machinery that colors `emit` (§2): the
    // keyed spelling joins the const-only set, because a runtime path reaching
    // it would compile clean and throw a `ReferenceError` on a helper with no
    // runtime binding (B143's shape).
    assert_fails_spanning(
        r#"
        import std::asset::emit_keyed;
        fun main() {
            emit_keyed("routes", "0", "x");
        }
        main();
        "#,
        r#"emit_keyed("routes", "0", "x")"#,
        "compile-time-only",
    );
}

#[test]
fn a_runtime_call_reaching_emit_keyed_is_named_for_the_spelling() {
    // The R-fixpoint names WHICH builtin the path reaches, and the keyed
    // spelling is its own name there — a reader told `asset::emit` would go
    // looking for a call that isn't in the file.
    assert_fails_with(
        r#"
        import std::asset::emit_keyed;
        fun contribute(): i32 {
            emit_keyed("routes", "0", "x");
            1
        }
        fun main() {
            let _x = contribute();
        }
        main();
        "#,
        "`contribute` (it reaches `asset::emit_keyed`) is compile-time-only",
    );
}

#[test]
fn a_function_reaching_emit_keyed_cannot_escape_as_a_value() {
    // The value-form half of §2: a call THROUGH a value has no statically known
    // callee, so the refusal sits where the value is made.
    assert_fails_with(
        r#"
        import std::asset::emit_keyed;
        fun contribute(): i32 {
            emit_keyed("routes", "0", "x");
            1
        }
        fun apply(f: || i32): i32 {
            f()
        }
        fun main() {
            let _x = apply(contribute);
        }
        main();
        "#,
        "no runtime value form",
    );
}

#[test]
fn emit_keyed_inside_a_const_stays_legal_through_a_value() {
    // The green negative for the fence: inside a `const` the interpreter makes
    // the call, so the restriction lifts for the keyed spelling exactly as it
    // does for `emit`, and the contribution still flows.
    let assembled = assembled_assets(
        r#"
        import std::io::print;
        import std::asset::emit_keyed;
        fun contribute(): i32 {
            emit_keyed("routes", "0", "x");
            1
        }
        fun apply(f: || i32): i32 {
            f()
        }
        fun main() {
            print(const apply(contribute));
        }
        main();
        "#,
    );
    assert_eq!(assembled.get("routes").map(String::as_str), Some("x\n"));
}

#[test]
fn a_runtime_read_is_rejected() {
    // The const-only bit, same machinery as `emit`'s (const-eval.md §2).
    assert_fails_spanning(
        r#"
        import std::asset;
        fun main() {
            let _text = asset::read("page.md");
        }
        main();
        "#,
        r#"asset::read("page.md")"#,
        "compile-time-only",
    );
}

#[test]
fn a_runtime_call_reaching_read_is_rejected_at_the_boundary() {
    // The R-fixpoint names WHICH builtin the path reaches — a read-reaching
    // function says `asset::read`, not `asset::emit`.
    assert_fails_with(
        r#"
        import std::asset;
        fun page(): str {
            asset::read("page.md")
        }
        fun main() {
            let _text = page();
        }
        main();
        "#,
        "`page` (it reaches `asset::read`) is compile-time-only",
    );
}

#[test]
fn a_function_reaching_read_cannot_escape_as_a_value() {
    assert_fails_with(
        r#"
        import std::asset;
        fun page(): str {
            asset::read("page.md")
        }
        fun apply(f: || str): str {
            f()
        }
        fun main() {
            let _text = apply(page);
        }
        main();
        "#,
        "no runtime value form",
    );
}

#[test]
fn a_changed_input_is_seen_by_the_next_analysis() {
    // The invalidation pin: analyze, EDIT THE FILE, analyze again in the same
    // process — the second analysis must fold the new content. Nothing in the
    // pipeline (the parse cache, the base cache, the shared const world) may
    // serve the first read's value for the second analysis; if any cache ever
    // keys const results without the read inputs, this goes red.
    let dir = std::env::temp_dir().join(format!("vilan-const-read-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = r#"
        import std::asset;
        fun main() {
            let _text = const asset::read("note.txt");
        }
        main();
        "#;
    std::fs::write(dir.join("note.txt"), "one").unwrap();
    let (first, _) = const_reads(source, &dir);
    assert_eq!(
        first,
        vec![vilan_core::interpreter::ConstValue::Str("one".to_string())]
    );
    std::fs::write(dir.join("note.txt"), "two").unwrap();
    let (second, inputs) = const_reads(source, &dir);
    assert_eq!(
        second,
        vec![vilan_core::interpreter::ConstValue::Str("two".to_string())],
        "the edited input must be seen — a stale read is a correctness bug"
    );
    assert!(
        inputs.len() == 1 && inputs[0].1.is_some(),
        "the re-read is recorded too: {inputs:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_read_bigger_than_the_fuel_budget_is_a_budget_miss() {
    // Reads charge fuel per byte, so the budget bounds input size exactly as
    // it bounds computation — without this, a read was one fuel tick and the
    // budget bounded nothing about it.
    let dir = std::env::temp_dir().join(format!("vilan-const-read-fuel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Comfortably past the explicit fuel budget in bytes.
    std::fs::write(dir.join("huge.txt"), "a".repeat(17_000_000)).unwrap();
    let source = r#"
        import std::asset;
        fun main() {
            let _text = const asset::read("huge.txt");
        }
        main();
        "#
    .to_string();
    let root = dir.clone();
    let messages = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (_, errors) = analyze_source(
                leaked,
                &std_spec(),
                &root,
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            errors
                .into_iter()
                .map(|error| error.msg)
                .collect::<Vec<_>>()
        })
        .unwrap()
        .join()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        messages.iter().any(|message| message
            .contains("did not finish within the compile-time budget")
            && message.contains("the fuel budget was exhausted")),
        "an oversized read must be a budget miss, with the remedy-naming \
         diagnostic intact: {messages:#?}"
    );
}

// --- B158: a blanket impl (`impl type T with Trait<T>`) reached through a  ---
// --- BOUND. Declared, it analyzed cleanly; the first dispatch through it   ---
// --- died with B55's internal error, because the emission-side re-dispatch ---
// --- matched impl subjects by nominal head and could not see a generic     ---
// --- subject at all. The rule that resolves it is SPECIFICITY              ---
// --- (`spec/types.md` §5.4): a blanket is the least-specific tier.         ---

/// B157's probe program, verbatim — the exhibit B158 was filed from. One
/// trait, one blanket static impl, one generic `Signal` impl, and a bounded
/// generic taking both a `str` and a `Signal<str>` with no call-site ceremony.
/// The `Signal` leg must REACT (the second `badge` line comes from `set`),
/// which is what makes the trait route answer B157's first problem.
#[test]
fn b158_the_maybe_signal_probe_dispatches_through_a_blanket_and_a_signal_impl() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::Signal;

        trait MaybeSignal<T> {
            fun bind(self, react: |T| void);
        }

        impl type T with MaybeSignal<T> {
            fun bind(self, react: |T| void) {
                react(self);
            }
        }

        impl Signal<type T> with MaybeSignal<T> {
            fun bind(self, react: |T| void) {
                let _watching = self.sub(react);
            }
        }

        fun badge<V: MaybeSignal<str>>(label: V) {
            label.bind(|text| print(i"[{text}]"));
        }

        fun count<V: MaybeSignal<i32>>(value: V) {
            value.bind(|n| print(i"n={n}"));
        }

        fun main() {
            badge("static");
            let live = Signal::new("first");
            badge(live);
            live.set("second");
            count(41);
            let n = Signal::new(1);
            count(n);
            n.set(2);
        }
        main();
        "#,
        "[static]\n[first]\n[second]\nn=41\nn=1\nn=2\n",
    );
}

/// A blanket ALONE — no constructor-headed impl to fall back on — answers a
/// bounded call for every type the bound admits. This is the shape that ICEd
/// most bluntly: nothing else could have answered, and nothing did.
#[test]
fn b158_a_blanket_alone_answers_a_bounded_call_for_every_type() {
    assert_compiles_and_runs(
        r#"

        trait Show2 { fun show2(self): str; }

        impl type T with Show2 { fun show2(self): str { "blanket" } }

        fun tell<V: Show2>(v: V) { print(v.show2()); }

        fun main() {
            tell("x");
            tell(3);
        }
        main();
        "#,
        "blanket\nblanket\n",
    );
}

/// The specificity rule itself, through a bound: a value matching BOTH a
/// blanket and a constructor-headed impl takes the constructor one. The
/// direct-receiver spelling already answered this (B73's R3); the bounded
/// spelling could not, and reached the trait's body-less requirement.
#[test]
fn b158_a_constructor_headed_impl_outranks_a_blanket_through_a_bound() {
    assert_compiles_and_runs(
        r#"

        trait Show2 { fun show2(self): str; }
        struct Bag {}

        impl type T with Show2 { fun show2(self): str { "blanket" } }
        impl Bag with Show2 { fun show2(self): str { "specific" } }

        fun tell<V: Show2>(v: V) { print(v.show2()); }

        fun main() {
            let bag = Bag {};
            print(bag.show2());
            tell(bag);
            tell("x");
        }
        main();
        "#,
        "specific\nspecific\nblanket\n",
    );
}

/// The case the owner's proposed `not Signal` bound would have DELETED
/// (B158's follow-up analysis): under a `MaybeSignal<Signal<str>>` bound, a
/// bare `Signal<str>` is a legitimate STATIC value, reached through the
/// blanket — while the same value under a `MaybeSignal<str>` bound is the
/// reactive source, reached through the constructor impl. The bound decides
/// which INSTANTIATION is wanted before specificity is consulted, so both
/// readings of one type stay reachable in one program.
#[test]
fn b158_a_nested_bound_reaches_the_blanket_for_a_value_the_signal_impl_also_matches() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::Signal;

        trait MaybeSignal<T> {
            fun bind(self, react: |T| void);
        }

        impl type T with MaybeSignal<T> {
            fun bind(self, react: |T| void) { react(self); }
        }

        impl Signal<type T> with MaybeSignal<T> {
            fun bind(self, react: |T| void) { let _watching = self.sub(react); }
        }

        fun badge<V: MaybeSignal<str>>(label: V) {
            label.bind(|text| print(i"badge {text}"));
        }

        fun holder<V: MaybeSignal<Signal<str>>>(slot: V) {
            slot.bind(|inner| print(i"holder {inner.get()}"));
        }

        fun main() {
            let live = Signal::new("one");
            badge(live);
            live.set("two");
            holder(live);
        }
        main();
        "#,
        "badge one\nbadge two\nholder two\n",
    );
}

/// The bounds tier of the same order, through a bound — a silent WRONG BODY
/// before B158, not an internal error: the direct receiver took
/// `Box2<type T: Marker>` and the bounded call took `Box2<type T>`, because
/// the emission side ranked nothing and answered in declaration order.
#[test]
fn b158_a_stronger_binder_bound_outranks_through_a_bound() {
    assert_compiles_and_runs(
        r#"

        trait Marker { fun marker(self): str; }
        trait Show2 { fun show2(self): str; }
        struct Box2<T> { v: T }
        struct Foo {}
        impl Foo with Marker { fun marker(self): str { "m" } }

        impl Box2<type T> with Show2 { fun show2(self): str { "plain" } }
        impl Box2<type T: Marker> with Show2 { fun show2(self): str { "marked" } }

        fun tell<V: Show2>(v: V) { print(v.show2()); }

        fun main() {
            let b = Box2 { v = Foo {} };
            print(b.show2());
            tell(b);
        }
        main();
        "#,
        "marked\nmarked\n",
    );
}

/// §13.2 row 17 through a BOUND: an impl that declares nothing and inherits
/// its trait's default still outranks a blanket that declares the name, in
/// either declaration order. The specificity order ranks IMPLS, not
/// declarations, so an inheriting winner answers with the default.
#[test]
fn b158_an_inheriting_impl_outranks_a_declaring_blanket_through_a_bound() {
    let program = |blanket_first: bool| {
        let impls = match blanket_first {
            true => {
                "impl type T with Tag { fun tag(self): i32 { 1 } }\n\
                     impl Foo with Tag { }"
            }
            false => {
                "impl Foo with Tag { }\n\
                      impl type T with Tag { fun tag(self): i32 { 1 } }"
            }
        };
        format!(
            r#"

            trait Tag {{ fun tag(self): i32 {{ 9 }} }}
            struct Foo {{ n: i32 }}

            {impls}

            fun tell<V: Tag>(v: V) {{ print(v.tag()); }}

            fun main() {{ tell(Foo {{ n = 1 }}); }}
            main();
            "#
        )
    };
    assert_compiles_and_runs(&program(true), "9\n");
    assert_compiles_and_runs(&program(false), "9\n");
}

/// Const evaluation reaches trait dispatch, so it reaches the blanket — and
/// must reach the SAME body the runtime path does, at both tiers.
#[test]
fn b158_const_evaluation_selects_the_same_impl_a_runtime_call_does() {
    assert_compiles_and_runs(
        r#"

        trait Label { fun label(self): str; }
        struct Tagged { n: i32 }

        impl type T with Label { fun label(self): str { "any" } }
        impl Tagged with Label { fun label(self): str { "tagged" } }

        fun describe<V: Label>(v: V): str { v.label() }

        fun main() {
            print(const describe(7));
            print(const describe(Tagged { n = 1 }));
            print(describe(9));
            print(describe(Tagged { n = 2 }));
        }
        main();
        "#,
        "any\ntagged\nany\ntagged\n",
    );
}

/// The overlap refusal, at the DECLARATION: two blankets of one trait have no
/// specificity order at any type, so the second is refused where it is
/// written, named, with the fix — never accepted and then answered by
/// declaration order.
#[test]
fn b158_a_second_blanket_of_one_trait_is_refused_at_its_declaration() {
    assert_fails_with(
        r#"

        trait Show2 { fun show2(self): str; }

        impl type T with Show2 { fun show2(self): str { "one" } }
        impl type U with Show2 { fun show2(self): str { "two" } }

        fun main() { print("x".show2()); }
        main();
        "#,
        "'Show2' is already implemented for 'U'; remove or merge this impl",
    );
}

/// Two BOUNDED blankets are two overlapping impls, not one written twice, so
/// they stand — until a type satisfying both reaches them. Through a bound
/// that landed in B55's internal error ("please report this program") for a
/// program that was the author's to fix; it is now the same refusal the
/// direct-receiver spelling gives, naming both subjects and the fix.
#[test]
fn b158_unrankable_overlapping_impls_are_refused_at_the_bound_that_reaches_them() {
    let source = r#"

        trait Alpha { fun alpha(self): str; }
        trait Beta { fun beta(self): str; }
        trait Show2 { fun show2(self): str; }

        impl type T: Alpha with Show2 { fun show2(self): str { "via-alpha" } }
        impl type U: Beta with Show2 { fun show2(self): str { "via-beta" } }

        struct Both {}
        impl Both with Alpha { fun alpha(self): str { "a" } }
        impl Both with Beta { fun beta(self): str { "b" } }

        fun tell<V: Show2>(v: V) { print(v.show2()); }

        fun main() { tell(Both {}); }
        main();
        "#;
    assert_fails_with(
        source,
        "'show2' cannot be dispatched on 'Both' through this call's 'Show2' bound",
    );
    assert_fails_without(source, "please report this program");
}

/// The same pair with only ONE of the two bounds satisfied is ranked by
/// applicability alone and answers — the refusal above must not spread to
/// every program that writes two conditional blankets.
#[test]
fn b158_two_bounded_blankets_still_answer_where_only_one_applies() {
    assert_compiles_and_runs(
        r#"

        trait Alpha { fun alpha(self): str; }
        trait Beta { fun beta(self): str; }
        trait Show2 { fun show2(self): str; }

        impl type T: Alpha with Show2 { fun show2(self): str { "via-alpha" } }
        impl type U: Beta with Show2 { fun show2(self): str { "via-beta" } }

        struct OnlyAlpha {}
        impl OnlyAlpha with Alpha { fun alpha(self): str { "a" } }
        struct OnlyBeta {}
        impl OnlyBeta with Beta { fun beta(self): str { "b" } }

        fun tell<V: Show2>(v: V) { print(v.show2()); }

        fun main() {
            tell(OnlyAlpha {});
            tell(OnlyBeta {});
        }
        main();
        "#,
        "via-alpha\nvia-beta\n",
    );
}

// --- B164: a supertrait's type argument is substituted through a sub-trait ---
// --- bound, so a member inherited from the supertrait is typed at the      ---
// --- arguments the chain passes it, not at its own abstract parameter.     ---
// Before the fix the member set of a bound was resolved by walking the trait
// and its supertraits for the NAME, then substituting the SUB-trait's
// parameters over whatever was found. A member declared by the supertrait is
// written in the supertrait's parameters, which that substitution never
// mentions, so the leaked parameter behaved as a wildcard and unified with
// whatever the call site claimed.

// The item's own shape: `get` comes from `Src<T>` under a `Sig<u32>` bound,
// and the function claims it returns `str`.
#[test]
fn a_supertrait_member_is_typed_at_the_sub_bounds_argument() {
    assert_fails_with(
        r#"
        trait Src<T> { fun get(self): T; }
        trait Sig<T> with Src<T> { fun set(self, value: T); }

        struct C { v: u32 }
        impl C with Src<u32> { fun get(self): u32 { self.v } }
        impl C with Sig<u32> { fun set(self, value: u32) { } }

        fun bad<S: Sig<u32>>(s: S): str { s.get() }

        fun main() {
        	let _ = bad(C { v = 7 });
        }
        "#,
        "Expected str, but got u32 instead.",
    );
}

// Two supertrait levels: the argument threads `Top` -> `Mid` -> `Base`.
#[test]
fn a_supertrait_argument_threads_two_levels_deep() {
    assert_fails_with(
        r#"
        trait Base<T> { fun get(self): T; }
        trait Mid<T> with Base<T> { fun mid(self); }
        trait Top<T> with Mid<T> { fun top(self); }

        struct C { v: u32 }
        impl C with Base<u32> { fun get(self): u32 { self.v } }
        impl C with Mid<u32> { fun mid(self) { } }
        impl C with Top<u32> { fun top(self) { } }

        fun deep<S: Top<u32>>(s: S): str { s.get() }

        fun main() {
        	let _ = deep(C { v = 7 });
        }
        "#,
        "Expected str, but got u32 instead.",
    );
}

// The parameters are matched by POSITION, not by name: a supertrait written
// with a different spelling of the parameter substitutes the same way.
#[test]
fn a_supertrait_with_a_different_parameter_spelling_still_substitutes() {
    assert_fails_with(
        r#"
        trait Src<A> { fun get(self): A; }
        trait Sig<A> with Src<A> { fun set(self, value: A); }

        struct C { v: u32 }
        impl C with Src<u32> { fun get(self): u32 { self.v } }
        impl C with Sig<u32> { fun set(self, value: u32) { } }

        fun bad<S: Sig<u32>>(s: S): str { s.get() }

        fun main() {
        	let _ = bad(C { v = 7 });
        }
        "#,
        "Expected str, but got u32 instead.",
    );
}

// The supertrait takes the sub-trait's SECOND parameter: the walk substitutes
// the supertrait's written arguments, so which slot they came from is the
// sub-trait's business and not the lookup's.
#[test]
fn a_supertrait_taking_a_later_parameter_substitutes_that_one() {
    assert_fails_with(
        r#"
        trait Src<T> { fun get(self): T; }
        trait Sig<A, B> with Src<B> { fun set(self, value: A); }

        struct C { v: u32 }
        impl C with Src<u32> { fun get(self): u32 { self.v } }
        impl C with Sig<str, u32> { fun set(self, value: str) { } }

        fun bad<S: Sig<str, u32>>(s: S): str { s.get() }

        fun main() {
        	let _ = bad(C { v = 7 });
        }
        "#,
        "Expected str, but got u32 instead.",
    );
}

// A supertrait named at a CONCRETE argument mentions none of the sub-trait's
// parameters, and is typed at that argument whatever the bound says.
#[test]
fn a_supertrait_at_a_concrete_argument_is_typed_at_it() {
    assert_fails_with(
        r#"
        trait Src<T> { fun get(self): T; }
        trait Sig<T> with Src<u32> { fun set(self, value: T); }

        struct C { v: u32 }
        impl C with Src<u32> { fun get(self): u32 { self.v } }
        impl C with Sig<str> { fun set(self, value: str) { } }

        fun bad<S: Sig<str>>(s: S): str { s.get() }

        fun main() {
        	let _ = bad(C { v = 7 });
        }
        "#,
        "Expected str, but got u32 instead.",
    );
}

// The control that fixes the boundary: a DIRECT bound on the supertrait, and
// the sub-trait's OWN member under a sub-trait bound, both substituted
// correctly before B164 and still do.
#[test]
fn a_direct_supertrait_bound_substitutes_as_it_always_did() {
    assert_fails_with(
        r#"
        trait Src<T> { fun get(self): T; }

        struct C { v: u32 }
        impl C with Src<u32> { fun get(self): u32 { self.v } }

        fun bad<S: Src<u32>>(s: S): str { s.get() }

        fun main() {
        	let _ = bad(C { v = 7 });
        }
        "#,
        "Expected str, but got u32 instead.",
    );
}

#[test]
fn a_sub_traits_own_member_substitutes_as_it_always_did() {
    assert_fails_with(
        r#"
        trait Src<T> { fun get(self): T; }
        trait Sig<T> with Src<T> { fun set(self, value: T); }

        struct C { v: u32 }
        impl C with Src<u32> { fun get(self): u32 { self.v } }
        impl C with Sig<u32> { fun set(self, value: u32) { } }

        fun writes<S: Sig<u32>>(s: S) { s.set("not a number"); }

        fun main() {
        	writes(C { v = 7 });
        }
        "#,
        "Expected u32, but got str instead.",
    );
}

// The green side, and the shape the hole surfaced as in practice: calling a
// method ON the result used to fail with `cannot call method 'to_string' on
// T`, because the leaked parameter was what the call landed on. It now types
// as `u32` and runs, and arithmetic on it agrees.
#[test]
fn a_supertrait_member_under_a_sub_bound_resolves_and_runs() {
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        trait Base<T> { fun get(self): T; }
        trait Mid<T> with Base<T> { fun mid(self); }
        trait Top<T> with Mid<T> { fun top(self); }

        struct C { v: u32 }
        impl C with Base<u32> { fun get(self): u32 { self.v } }
        impl C with Mid<u32> { fun mid(self) { } }
        impl C with Top<u32> { fun top(self) { } }

        fun via_sub<S: Top<u32>>(s: S): u32 { s.get() }
        fun shows<S: Top<u32>>(s: S) { print(s.get().to_string()); }

        fun main() {
        	print(via_sub(C { v = 7 }) + 1u32);
        	shows(C { v = 2 });
        }
        "#,
        "8\n2\n",
    );
}

// ── A33: a read-only binding demands `Source`, not `Signal` ───────────────────
//
// Every `bind_*` in `std::ui` took the CONCRETE `Signal`, so a user type that
// implements `Source<T>` — kolt's `StorageSignal`, a `RemoteSource`, any custom
// mirror — could not drive a binding it only ever reads from. A33 swept std,
// classified each site READ-ONLY or WRITE-BACK, and widened the read-only ones
// to a `Source<T>` bound in BOTH `ui` twins.
//
// The exhibit below is `StorageSignal` in miniature: a struct wrapping a
// `Signal`, implementing `Source` by delegation, with `set` kept OFF the trait —
// so a binding that reached for `set` would not compile against it, which is
// what makes the write-back refusals further down non-vacuous.

/// The acceptance exhibit's type, shared by the pins in this block.
const A_USER_SOURCE: &str = r#"
        struct Stored<T> { inner: Signal<T> }

        impl Stored<type T> with Source<T> {
            fun get(self): T { self.inner.get() }
            [must_use]
            fun sub(self, observer: |T| void): Subscription { self.inner.sub(observer) }
        }

        impl Stored<type T> {
            fun new(value: T): Stored<T> { Stored { inner = Signal::new(value) } }
            fun set(self, value: T) { self.inner.set(value); }
        }
"#;

/// Every widened binding on the BROWSER twin, driven by a user `Source`. Before
/// A33 each of these was a type error naming `Signal`.
#[test]
fn a_user_source_drives_every_read_only_browser_binding() {
    assert_compiles_browser(&format!(
        r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::style::{{ Color, Style, style }};
        import std::ui::{{ View, mount_root, view }};
        {A_USER_SOURCE}
        fun main() {{
            let label: Stored<str> = Stored::new("alpha");
            let flag: Stored<bool> = Stored::new(true);
            let items: Stored<List<str>> = Stored::new(["x", "y"]);
            let skin: Stored<Style> = Stored::new(const style().color(Color::gray(500)));
            let _root = mount_root("app", || view("main")
                .child(view("h1").bind_text(label))
                .child(view("p").bind_class(label))
                .child(view("a").bind_attr("href", label))
                .child(view("div").style_var("--w", label))
                .child(view("span").bind_styled(skin))
                .child(view("i").show(flag))
                .child(view("ul").bind_each(items, |item| item, |item| view("li").text(item)))
                .child(view("aside").when(flag, || view("b").text("here"))));
        }}
        "#
    ));
}

/// The same surface on the PROCESS twin, and it renders: the two `ui` halves
/// widened in lockstep, so a custom-source component is not client-only. The
/// markup is the claim — a read-once binding that dropped its value would
/// serve empty attributes and pass a compile-only pin.
#[test]
fn a_user_source_drives_the_process_twin_and_renders() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ View, render, view }};
        {A_USER_SOURCE}
        fun main() {{
            let label: Stored<str> = Stored::new("alpha");
            let items: Stored<List<str>> = Stored::new(["x", "y"]);
            let shown: Stored<bool> = Stored::new(true);
            let hidden: Stored<bool> = Stored::new(false);
            print(render(view("main")
                .child(view("h1").bind_text(label))
                .child(view("p").bind_class(label))
                .child(view("a").bind_attr("href", label))
                .child(view("div").style_var("--w", label))
                .child(view("i").show(hidden))
                .child(view("ul").bind_each(items, |item| item, |item| view("li").text(item)))
                .child(view("aside").when(shown, || view("b").text("here")))));
        }}
        main();
        "#
        ),
        "<main><h1>alpha</h1><p class=\"alpha\"></p><a href=\"alpha\"></a>\
         <div style=\"--w:alpha\"></div><i hidden=\"\"></i>\
         <ul><li>x</li><li>y</li></ul><aside><b>here</b></aside></main>\n",
    );
}

/// The HOLD, half one: `bind_value` WRITES back, so it keeps its concrete
/// `Signal` until A32 rules on the write side. A `Source` has no `set`, so this
/// is a refusal and not an oversight — and pinning it means a later blanket
/// widening cannot take the write side by accident.
#[test]
fn bind_value_still_demands_a_signal() {
    assert_fails_browser_with(
        &format!(
            r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ View, mount_root, view }};
        {A_USER_SOURCE}
        fun main() {{
            let typed: Stored<str> = Stored::new("");
            let _root = mount_root("app", || view("input").bind_value(typed));
        }}
        "#
        ),
        "Signal",
    );
}

/// The HOLD, half two: `bind_draft` pushes through a `Draft`, which is the
/// write side by construction.
#[test]
fn bind_draft_still_demands_a_draft() {
    assert_fails_browser_with(
        &format!(
            r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ View, mount_root, view }};
        {A_USER_SOURCE}
        fun main() {{
            let typed: Stored<str> = Stored::new("");
            let _root = mount_root("app", || view("input").bind_draft(typed));
        }}
        "#
        ),
        "Draft",
    );
}

/// The B158 DEFERRAL, stated as a test rather than only as a comment.
/// `attr`/`child` dispatch through the `AttrValue`/`Slot` TRAITS, whose tracked
/// arms are `impl Signal<str> with …`. Widening a trait arm is a BLANKET impl
/// (`impl S: Source<str> with AttrValue`), which is B158's machinery, not a
/// bound on a generic parameter — so a user source is still refused here while
/// it drives every `bind_*`. Un-ignore nothing when B158 lands: replace this
/// pin with its positive twin.
#[test]
fn the_attr_and_slot_trait_arms_are_still_signal_only() {
    let program = |value: &str| {
        format!(
            r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ View, mount_root, view }};
        {A_USER_SOURCE}
        fun main() {{
            let label: Stored<str> = Stored::new("alpha");
            let _root = mount_root("app", || {value});
        }}
        "#
        )
    };
    assert_fails_browser_with(&program(r#"view("a").attr("href", label)"#), "AttrValue");
    assert_fails_browser_with(&program(r#"view("p").child(label)"#), "Slot");
}

/// …and the concrete arms are untouched, which is the no-regression half: the
/// element syntax desugars an attribute to `.attr(name, value)` and a hole to
/// `.child(value)`, never to `bind_attr`/`bind_text`, so the widening must not
/// have moved what `<div id(signal)>` and `<p>{signal}</p>` resolve to.
#[test]
fn element_syntax_still_routes_attributes_through_attr_value() {
    assert_compiles_browser(
        r#"
        import std::reactive::Signal;
        import std::ui::{ View, mount_root, view };

        fun main() {
            let label = Signal::new("alpha");
            let _root = mount_root("app", || <div id(label) class("static")>{label}</div>);
        }
        "#,
    );
}

// ── B168: the three A33 held back, widened ───────────────────────────────────
//
// A33's sweep classified `View::swap` READ-ONLY like every other binding it
// widened, and held it anyway: a `Source<T>` bound whose argument is a BARE
// generic parameter lost its link to the caller's `T` inside a generic body,
// and `swap_split` calls `self.swap(gated, render)` from exactly such a body.
// B168 fixed that reconciliation (`tests/inference/generics.rs` carries the
// minimized pin and its edge cases), so the three that waited — `View::swap`,
// `View::swap_split` and `ui::chunk_preload` — widened together, in
// generic-parameter ORDER, which is what the split gate rebinds by position.
//
// Each pin below drives its signature with `A_USER_SOURCE`: a type that is a
// `Source` and is NOT a `Signal`. Against the held signatures every one of them
// was a type error naming `Signal`.

/// `View::swap` on the BROWSER twin, driven by a user `Source`.
#[test]
fn a_user_source_drives_the_browser_swap() {
    assert_compiles_browser(&format!(
        r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ View, mount_root, view }};
        {A_USER_SOURCE}
        fun main() {{
            let route: Stored<str> = Stored::new("home");
            let _root = mount_root("app", || view("main")
                .swap(route, |current| view("section").text(current)));
        }}
        "#
    ));
}

/// `View::swap_split` — the split build's gate — driven by the same user
/// `Source`. It is the signature whose own BODY carries the B168 shape
/// (`self.swap(gated, render)` with `gated: Signal<T>`), so this pin is the
/// compiler fix read through std rather than through a minimized exhibit.
#[test]
fn a_user_source_drives_the_split_gate() {
    assert_compiles_browser(&format!(
        r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ View, mount_root, view }};
        {A_USER_SOURCE}
        fun main() {{
            let route: Stored<str> = Stored::new("home");
            let _root = mount_root("app", || view("main")
                .swap_split(route, |current| view("section").text(current)));
        }}
        "#
    ));
}

/// `ui::chunk_preload` — the boot preload the emitter plants in front of the
/// gate call — driven by the same user `Source`. It declares the same generics
/// as `swap_split` in the same order, and this pin is what keeps the pair
/// spelled alike after the widening.
#[test]
fn a_user_source_drives_the_boot_preload() {
    assert_compiles_browser(&format!(
        r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ chunk_preload, View, mount_root, view }};
        {A_USER_SOURCE}
        fun main() {{
            let route: Stored<str> = Stored::new("home");
            chunk_preload(route);
            let _root = mount_root("app", || view("main").text("shell"));
        }}
        "#
    ));
}

/// The PROCESS twin's `swap`, and it renders: the two `ui` halves widened in
/// lockstep here as they did for A33's eight, so a custom-source route is not
/// client-only. The markup is the claim — a widened signature that dropped the
/// value would serve an empty section and pass a compile-only pin.
#[test]
fn a_user_source_drives_the_process_swap_and_renders() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::reactive::{{ Signal, Source, Subscription }};
        import std::ui::{{ View, render, view }};
        {A_USER_SOURCE}
        fun main() {{
            let route: Stored<str> = Stored::new("docs");
            print(render(view("main")
                .swap(route, |current| view("section").text(current))));
        }}
        main();
        "#
        ),
        "<main><section>docs</section></main>\n",
    );
}

/// The no-regression half: a concrete `Signal` still drives all three. `Signal`
/// implements `Source`, so the widening must have kept every existing call site
/// — `std::router`'s `swap(route, ..)` is one, and the split fixture's gate is
/// another.
#[test]
fn a_concrete_signal_still_drives_the_swap_family() {
    assert_compiles_browser(
        r#"
        import std::reactive::Signal;
        import std::ui::{ chunk_preload, View, mount_root, view };

        fun main() {
            let route = Signal::new("home");
            chunk_preload(route);
            let _root = mount_root("app", || view("main")
                .swap(route, |current| view("section").text(current))
                .swap_split(route, |current| view("aside").text(current)));
        }
        "#,
    );
}

// ---------------------------------------------------------------------------
// B165 — a `type` binder inside an impl head's BOUND
// ---------------------------------------------------------------------------

/// The shape B157's generic blanket needs: the subject's binder is constrained
/// by a PARAMETERIZED trait, and the bound's own argument is a fresh binder the
/// rest of the head reuses. `type T` under `Src<..>` used to report `cannot
/// find type 'T'` — the binder was registered for the subject alone, never for
/// the bound it is written inside.
#[test]
fn b165_a_type_binder_inside_a_bound_declares_the_impls_parameter() {
    assert_compiles_and_runs(
        r#"

        trait Src<T> { fun read(self): T; }
        trait Maybe<T> { fun show(self, react: |T| void); }

        struct Cell { value: str }
        impl Cell with Src<str> { fun read(self): str { self.value } }

        impl type S: Src<type T> with Maybe<T> {
            fun show(self, react: |T| void) { react(self.read()); }
        }

        fun tell<V: Maybe<str>>(v: V) { v.show(|text| print(text)); }

        fun main() {
            tell(Cell { value = "through the bound" });
        }
        main();
        "#,
        "through the bound\n",
    );
}

/// The binder is the IMPL's parameter, so it varies per receiver: two `Src`
/// impls at different arguments both reach the one blanket, each at its own
/// element type.
#[test]
fn b165_the_bounds_binder_varies_with_the_receivers_own_argument() {
    assert_compiles_and_runs(
        r#"

        trait Src<T> { fun read(self): T; }
        trait Maybe<T> { fun show(self, react: |T| void); }

        struct Words { value: str }
        struct Counts { value: i32 }
        impl Words with Src<str> { fun read(self): str { self.value } }
        impl Counts with Src<i32> { fun read(self): i32 { self.value } }

        impl type S: Src<type T> with Maybe<T> {
            fun show(self, react: |T| void) { react(self.read()); }
        }

        fun words<V: Maybe<str>>(v: V) { v.show(|text| print(text)); }
        fun counts<V: Maybe<i32>>(v: V) { v.show(|n| print(i"n={n}")); }

        fun main() {
            words(Words { value = "text" });
            counts(Counts { value = 7 });
        }
        main();
        "#,
        "text\nn=7\n",
    );
}

/// Binder scope is the WHOLE head, not just the `with` clause: a binder written
/// in one bound is reusable by a LATER bound on the same subject.
#[test]
fn b165_a_binder_from_one_bound_is_reusable_by_a_sibling_bound() {
    assert_compiles_and_runs(
        r#"

        trait Src<T> { fun read(self): T; }
        trait Tagged<T> { fun tag(self): T; }
        trait Maybe<T> { fun show(self, react: |T| void); }

        struct Cell { value: str }
        impl Cell with Src<str> { fun read(self): str { self.value } }
        impl Cell with Tagged<str> { fun tag(self): str { "cell" } }

        impl type S: Src<type T> + Tagged<T> with Maybe<T> {
            fun show(self, react: |T| void) { react(self.tag()); react(self.read()); }
        }

        fun tell<V: Maybe<str>>(v: V) { v.show(|text| print(text)); }

        fun main() { tell(Cell { value = "body" }); }
        main();
        "#,
        "cell\nbody\n",
    );
}

/// A name that is NOT declared as a binder anywhere in the head still refuses,
/// in the same words — the fix widens where a binder may be WRITTEN, it does
/// not make every name in a bound resolve.
#[test]
fn b165_an_undeclared_name_inside_a_bound_still_refuses() {
    assert_fails_with(
        r#"

        trait Src<T> { fun read(self): T; }
        trait Maybe<T> { fun show(self, react: |T| void); }

        impl type S: Src<Missing> with Maybe<Missing> {
            fun show(self, react: |Missing| void) { react(self.read()); }
        }
        "#,
        "cannot find type 'Missing'",
    );
}

/// The generic blanket standing beside the static one — B157's `MaybeSignal`
/// family, whole: a plain value reaches the blanket, a `Source` of the same
/// element reaches the bounded impl, and B158's specificity order picks between
/// them.
#[test]
fn b165_the_static_blanket_and_a_source_bounded_blanket_coexist() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ Signal, Source };

        trait Maybe<T> { fun show(self, react: |T| void); }

        impl type T with Maybe<T> {
            fun show(self, react: |T| void) { react(self); }
        }

        impl type S: Source<type T> with Maybe<T> {
            fun show(self, react: |T| void) { let _watching = self.sub(react); }
        }

        fun badge<V: Maybe<str>>(label: V) { label.show(|text| print(i"[{text}]")); }

        fun main() {
            badge("static");
            let live = Signal::new("first");
            badge(live);
            live.set("second");
        }
        main();
        "#,
        "[static]\n[first]\n[second]\n",
    );
}
