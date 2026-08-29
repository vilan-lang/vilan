//! Borrows, views and the place rules: the regression guards, transparent
//! references, wire-blessed handles, `mut` parameters, and the rule-1/rule-2
//! arc (B81, B88, B89, B94, B97, B99, B100, B104, B108/B109, B134, B64).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- Regression guards (must keep passing) ----------------------------------

#[test]
fn generic_method_calls_generic_methods_on_self() {
    // Bug A (fixed): `update` calls both `self.set` and `self.get` — two generic
    // method calls on the same receiver. This used to overflow the compiler.
    assert_compiles(
        r#"
        import std::shared::Shared;
        struct Cell<T> { value: Shared<T> }
        impl Cell<type T> {
            fun new(value: T): Cell<T> { Cell { value = Shared::new(value) } }
            fun get(self): T { self.value.read() }
            fun set(self, value: T) { self.value.write() = value; }
            fun update(self, f: |T| T) { self.set(f(self.get())); }
        }
        fun main() { let c = Cell::new(0); c.update(|n| n + 1); }
        "#,
    );
}

#[test]
fn reactive_map_sub_and_set_with() {
    assert_compiles(
        r#"
        import std::print;
        import std::reactive::{ Signal, Owner };
        fun main() {
            let owner = Owner::new();
            let count = Signal::new(0);
            let doubled = count.map(|n| n * 2);
            owner.take(doubled.sub(|n| print(n)));
            count.set_with(|n| n + 1);
        }
        "#,
    );
}

#[test]
fn owner_disposes_subscriptions_across_re_renders() {
    // A2: the leak fix. Mimics `bind_each` — `source` drives re-renders; each
    // render disposes the previous rows' subscriptions (`rows.dispose()`) and
    // creates fresh ones. After several renders only the *current* rows fire, so
    // the count stays bounded (a leak would give 6, not 2).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::shared::Shared;
        import std::reactive::{ Signal, Owner };
        fun main() {
            let source = Signal::new(0);
            let data = Signal::new(0);
            let rows = Owner::new();
            let fires = Shared::new(0);
            let outer = Owner::new();
            outer.take(source.sub(|_| {
                rows.dispose();
                rows.take(data.sub(|_| { fires.write() = fires.read() + 1; }));
                rows.take(data.sub(|_| { fires.write() = fires.read() + 1; }));
            }));
            source.set(1);
            source.set(2);
            fires.write() = 0;
            data.set(99);
            print(fires.read());
        }
        "#,
        "2\n",
    );
}

#[test]
fn generic_dispatch_to_extern_impl() {
    // A trait method on a generic, dispatching to a primitive's `[extern]` impl.
    assert_compiles(
        r#"
        import std::print;
        import std::display::{ Display, format };
        fun show<T: Display>(x: T): str { x.to_string() }
        fun main() { print(format(42)); print(show("hi")); }
        "#,
    );
}

#[test]
fn return_type_only_generic() {
    // A generic fixed only by the return type (no argument binds it).
    assert_compiles(
        r#"
        import std::print;
        import std::default::Default;
        fun make<T: Default>(): T { T::default() }
        fun main() { let n: i32 = make(); print(n); }
        "#,
    );
}

#[test]
fn collection_json_roundtrip() {
    assert_compiles(
        r#"
        import std::print;
        import std::json::{ Json, FromJson };
        import std::result::Result::{ self, Ok, Err };
        fun main() {
            let nums: Result<List<i32>, str> = List::from_json("[1,2,3]");
            print(nums is Ok(let ns) && ns.to_json() == "[1,2,3]");
        }
        "#,
    );
}

#[test]
fn nested_generic_containers() {
    // `Option<List<i32>>` etc. — generic args nested several deep must resolve.
    assert_compiles(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let x: Option<List<i32>> = Some([1, 2, 3]);
            match x {
                Some(let list) => print(list.len()),
                None => print(0),
            }
        }
        "#,
    );
}

#[test]
fn recursion_self_and_mutual() {
    assert_compiles(
        r#"
        import std::print;
        fun fib(n: i32): i32 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
        fun is_even(n: i32): bool { if n == 0 { true } else { is_odd(n - 1) } }
        fun is_odd(n: i32): bool { if n == 0 { false } else { is_even(n - 1) } }
        fun main() { print(fib(10)); print(is_even(4)); }
        "#,
    );
}

#[test]
fn calling_a_non_function_still_errors() {
    // A real error must still be reported (not silently swallowed).
    assert_fails(
        r#"
        struct Point { x: i32 }
        fun main() { let p = Point { x = 1 }; p(); }
        "#,
    );
}

#[test]
fn generic_struct_infers_type_arg_from_literal() {
    // A generic struct built by literal infers its parameter from the field
    // value (`Box { value = 5 }` -> `Box<i32>`), so a later method dispatches
    // against the concrete element. Previously the initializer dropped the
    // inferred arg (`Box<>`), leaving `T` abstract.
    assert_compiles(
        r#"
        import std::print;
        import std::display::Display;
        struct Box<T> { value: T }
        impl Box<type T> { fun get(self): T { self.value } }
        fun main() { let b = Box { value = 5 }; print(b.get().to_string()); }
        "#,
    );
}

#[test]
fn generic_struct_infers_type_arg_from_constructor() {
    // The same inference through a static constructor: `Box::new(5)` binds the
    // *impl's* `T` from the argument even though `new` declares no generics of
    // its own. (Bug B in disguise — `Signal::new(0).map(|n| ..)` left `n`
    // abstract only because `count` itself was an abstract `Signal<T>`.)
    assert_compiles(
        r#"
        import std::print;
        import std::display::Display;
        struct Box<T> { value: T }
        impl Box<type T> {
            fun new(value: T): Box<T> { Box { value = value } }
            fun get(self): T { self.value }
        }
        fun main() { print(Box::new(5).get().to_string()); }
        "#,
    );
}

#[test]
fn generic_call_on_closure_parameter() {
    // Bug B (fixed): a closure passed to a generic method (`count.map(|n|
    // n.to_string())`) used to type `n` as an abstract generic, so the method
    // call on it couldn't dispatch. The real cause was that `Signal::new(0)`
    // left `count` as an abstract `Signal<T>`; with construction now inferring
    // `Signal<i32>`, `n` is `i32` and `to_string` dispatches.
    assert_compiles(
        r#"
        import std::print;
        import std::reactive::Signal;
        import std::display::Display;
        fun main() {
            let count = Signal::new(0);
            let label = count.map(|n| n.to_string());
            label.sub(|s| print(s));
        }
        "#,
    );
}

#[test]
fn format_through_nested_generic() {
    // Bug C (fixed): a generic function passing its type parameter to another
    // generic call (`show<T: Display>(x) { format(x) }`) used to leave the nested
    // `format` un-monomorphized — its `value.to_string()` resolved to the empty
    // abstract `Display::to_string`, printing `undefined`. The cause was a binding
    // direction: the call reconciled argument-against-parameter, so a generic
    // argument bound *its own* constraint instead of the callee's. Reconciling
    // parameter-first binds `format`'s `U = T`, so it monomorphizes per `show`
    // instantiation.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::{ Display, format };
        fun show<T: Display>(x: T): str { format(x) }
        fun main() { print(show(7)); print(show("hi")); }
        "#,
        "7\nhi\n",
    );
}

#[test]
fn chained_derive_binds_method_generic_from_closure_return() {
    // A chained `derive` (`count.map(|n| n * 2).map(|m| format(m))`) used to
    // emit `undefined`: the first `derive<U>` left its result `Signal<U>` abstract
    // because `U` (its *own* generic) was never bound from the closure's return
    // type, so the second `derive` saw an abstract element. Method calls now bind
    // their own generics from arguments, like free-function calls do.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::reactive::Signal;
        import std::display::format;
        fun main() {
            let count = Signal::new(3);
            let label = count.map(|n| n * 2).map(|m| format(m));
            label.sub(|s| print(s));
            count.set(10);
        }
        "#,
        "6\n20\n",
    );
}

#[test]
fn format_in_closure_argument() {
    // Bug c′ (fixed): a free generic function called with an unannotated closure
    // parameter (`count.map(|n| format(n))`) emitted `undefined`. The call
    // resolved while `n` was still `Unknown` (its type lands only once `derive`
    // resolves), committed with no generic binding, and was never revisited.
    // Fixed by deferring the call while an argument is an unknown closure
    // parameter — the same rule the method-call resolver already applies to an
    // unknown closure *receiver* — so it re-resolves once `n` becomes `i32`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::reactive::Signal;
        import std::display::format;
        fun main() {
            let count = Signal::new(0);
            let label = count.map(|n| format(n));
            label.sub(|s| print(s));
            count.set(5);
        }
        "#,
        "0\n5\n",
    );
}

#[test]
fn method_closure_param_inferred_from_argument_generic() {
    // A method's own generic bound from a (nested) argument must reach its closure
    // parameters: `pick<T, K>(rows: List<List<T>>, key: |T| K, get: |T| i32)` typed
    // `|p| p.id`'s `p` as the abstract `T` until the own-generic binding ran first.
    // This is the `bind_each(source: Signal<List<T>>, |todo| todo.id, ..)` shape.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        struct P { id: i32 }
        struct Holder { tag: i32 }
        impl Holder {
            fun pick<T, K>(self, rows: List<List<T>>, key: |T| K, get: |T| i32): i32 {
                get(rows[0][0])
            }
        }
        fun main() {
            let h = Holder { tag = 0 };
            print(h.pick([[P { id = 42 }]], |p| p.id, |p| p.id).to_string());
        }
        "#,
        "42\n",
    );
}

#[test]
fn logical_or_operator() {
    // `||` is logical-or: binds looser than `&&`, short-circuits, and an empty
    // closure `|| body` still parses (it's tried before the operator).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun boom(): bool { print("evaluated"); true }
        fun main() {
            let a = "x";
            print(a == "x" || a == "y");
            print(a == "z" || a == "y");
            print(a == "x" && false || a == "x");
            print(true || boom());
            let f = || 7;
            print(f());
        }
        "#,
        "true\nfalse\ntrue\ntrue\n7\n",
    );
}

#[test]
fn reactive_combine_variadic() {
    // The driving example: `combine` is variadic over its inputs' distinct types
    // via a mapped-tuple parameter, yielding a `Signal` of the tuple that
    // recomputes when any input changes. The consumer destructures the tuple with
    // a closure tuple binder.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        import std::reactive::{ Signal, combine };
        fun main() {
            let a = Signal::new(1);
            let b = Signal::new("x");
            let c = Signal::new(true);
            let combined: Signal<(i32, str, bool)> = combine((a, b, c));
            combined.sub(|(n, s, flag)| print(i"{n.to_string()} {s} {flag}"));
            a.set(2);
            b.set("y");
        }
        "#,
        "1 x true\n2 x true\n2 y true\n",
    );
}

#[test]
fn tuple_comprehension_over_mapped_source() {
    // A tuple comprehension `(x in xs => e)` maps each element of a mapped-tuple
    // source through the body, typing as `(U in T: <body>)`. Here `source.len()`
    // collapses `(List<i32>, List<str>)` to `(i32, str) = T`. Lowers to a runtime
    // `.map`, so it's arity-independent.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun lengths<T: (2..)>(sources: (U in T: List<U>)): T {
            (source in sources => source.len())
        }
        fun main() {
            let (a, b) = lengths(([1, 2, 3], ["a", "b"]));
            print(i"{a.to_string()} {b.to_string()}");
        }
        "#,
        "3 2\n",
    );
}

#[test]
fn mapped_tuple_forward_expansion() {
    // A mapped tuple type with a concrete source expands element-wise:
    // `(U in (i32, str): List<U>)` is `(List<i32>, List<str>)`, so each binding
    // dispatches concretely.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun main() {
            let pair: (U in (i32, str): List<U>) = ([1, 2], ["x", "y", "z"]);
            let (nums, strs) = pair;
            print(i"{nums.len().to_string()} {strs.len().to_string()}");
        }
        "#,
        "2 3\n",
    );
}

#[test]
fn mapped_tuple_inverted_inference() {
    // A generic function over a mapped parameter infers the source tuple `T` from
    // the argument by inverting the template per element: `id(([1,2,3], ["a","b"]))`
    // binds `T = (i32, str)`, so the result mapped type re-expands to
    // `(List<i32>, List<str>)`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun id<T: (2..)>(sources: (U in T: List<U>)): (U in T: List<U>) { sources }
        fun main() {
            let (nums, strs) = id(([1, 2, 3], ["a", "b"]));
            print(i"{nums.len().to_string()} {strs.len().to_string()}");
        }
        "#,
        "3 2\n",
    );
}

#[test]
fn tuple_arity_bounds_parse() {
    // The tuple-bound grammar — `(..)`, `(2..)`, `(..10)`, and a per-element
    // bound `(2..: Display)` — parses and the parameter behaves as a generic
    // tuple. (Arity isn't enforced, mirroring trait bounds, which aren't either.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun any<T: (..)>(x: T): T { x }
        fun two<T: (2..)>(x: T): T { x }
        fun small<T: (..10)>(x: T): T { x }
        fun shown<T: (2..: Display)>(x: T): T { x }
        fun main() {
            let (a, b) = two((1, 2));
            let (c, d, e) = any((3, 4, 5));
            print(i"{a.to_string()} {b.to_string()} {c.to_string()} {d.to_string()} {e.to_string()}");
        }
        "#,
        "1 2 3 4 5\n",
    );
}

#[test]
fn nested_tuple_flat_lowering() {
    // A nested tuple stores flat (`((1,2),3)` -> `[1,2,3]`), so a matching nested
    // pattern reads flat offsets and a sub-tuple capture reslices — all behaviorally
    // transparent. Distinct types are preserved: the pattern must match the nesting.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun main() {
            let a = (1, 2);
            let b = (a, 3);
            let ((x, y), z) = b;
            print(i"{x.to_string()} {y.to_string()} {z.to_string()}");
            let (pair, last) = b;
            let (pa, pb) = pair;
            print(i"{pa.to_string()} {pb.to_string()} {last.to_string()}");
        }
        "#,
        "1 2 3\n1 2 3\n",
    );
}

#[test]
fn parameter_tuple_destructuring() {
    // A tuple binder in parameter position — both a function parameter
    // (`fun f((a, b): T)`) and a closure parameter (`|(a, b)|`) — destructures,
    // typing each binding from the matched tuple element.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun sum_pair((a, b): (i32, i32)): i32 { a + b }
        fun apply(pair: (i32, str), f: |(i32, str)| str): str { f(pair) }
        fun main() {
            print(sum_pair((3, 4)).to_string());
            print(apply((7, "x"), |(n, label)| i"{n.to_string()}{label}"));
        }
        "#,
        "7\n7x\n",
    );
}

#[test]
fn nested_parameter_tuple_destructuring() {
    // A nested tuple binder in a closure parameter, dispatched through a generic
    // reactive `derive` so the parameter type is inferred, not annotated.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun main() {
            let f = |(a, (b, c)): (i32, (i32, str))| i"{a.to_string()} {b.to_string()} {c}";
            print(f((1, (2, "z"))));
        }
        "#,
        "1 2 z\n",
    );
}

#[test]
fn let_tuple_destructuring() {
    // `let (a, b, c) = tuple` destructures, typing each binding from the tuple's
    // element types (so a method call on a binding dispatches concretely).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        fun pair(): (i32, str) { (7, "x") }
        fun main() {
            let (a, (b, c)) = (1, (2, 3));
            let (n, label) = pair();
            print(i"{a} {b} {c} {n.to_string()} {label}");
        }
        "#,
        "1 2 3 7 x\n",
    );
}

// --- Transparent references (implicit place, explicit value) ----------------

#[test]
fn transparent_references_write_through() {
    // R5: assigning *through* a view writes to its referent with no `*` — a view
    // binding, a `&mut` parameter, a re-borrow, a `borrows`-returning call, and a
    // captured `Option<&mut T>`, for plain `=` and compound `+=` / `/=`. Reading a
    // view as a value keeps its explicit `*`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun add_ten(x: &mut i32) { x += 10; }
        fun same(x: &mut i32): &mut i32 borrows x { x }
        struct Cell { value: i32 }
        impl Cell { fun slot(&mut self): Option<&mut i32> { Some(&mut self.value) } }
        fun main() {
            mut a: i32 = 10;
            let b: &mut i32 = &mut a;
            let c: &mut i32 = b;
            b = 20;
            print(i"{a} {*b} {*c}");
            add_ten(&mut a);
            print(i"{a} {*b}");
            add_ten(b);
            print(i"{a} {*b}");
            same(c) /= 10;
            print(i"{a} {*b}");
            mut cell = Cell { value = 100 };
            match cell.slot() {
                Some(let s) => { s += 5 }
                None => {}
            }
            print(cell.value);
        }
        "#,
        "20 20 20\n30 30\n40 40\n4 4\n105\n",
    );
}

#[test]
fn transparent_references_reject_deref_assignment() {
    // R6: `*` is value extraction (an rvalue) and may not be an assignment
    // target — write `v = …`, not `*v = …`.
    assert_fails(
        r#"
        fun main() { mut a = 5; let v: &mut i32 = &mut a; *v = 9; }
        "#,
    );
}

#[test]
fn transparent_references_reject_mut_view_binding() {
    // R7: a view binding cannot be `mut` — a view cannot be rebound.
    assert_fails(
        r#"
        fun main() { mut a = 5; mut v: &mut i32 = &mut a; v = 9; }
        "#,
    );
}

#[test]
fn transparent_references_reject_view_into_value_binding() {
    // R1: a value annotation cannot bind a view — write `*` to copy the value out.
    assert_fails(
        r#"
        fun main() { mut a = 5; let v: &mut i32 = &mut a; let b: i32 = v; }
        "#,
    );
}

#[test]
fn a_view_annotation_over_a_value_initializer_names_the_mismatch() {
    // R1's view-annotated arm, textually (ledger row 18): the annotation
    // promises a view and the initializer is a value.
    assert_fails_with(
        r#"
        fun main() { mut a = 5; let v: &mut i32 = 9; }
        "#,
        "'v' is annotated as a view (`&[mut] T`) but its initializer is not a view; bind a `&[mut] place` to alias it.",
    );
}

#[test]
fn a_value_annotation_over_a_view_initializer_names_the_mismatch() {
    // R1's value-annotated arm, textually (ledger row 18) — the textual twin
    // of transparent_references_reject_view_into_value_binding above.
    assert_fails_with(
        r#"
        fun main() { mut a = 5; let v: &mut i32 = &mut a; let b: i32 = v; }
        "#,
        "'b' is annotated as a value but its initializer is a view; write `*` to copy the value out, or annotate `&[mut] T` to alias it.",
    );
}

#[test]
fn an_inline_option_view_transient_writes_through() {
    // C5.2: constructing an `Option<&mut T>` inline and immediately matching it —
    // the transient the spec's open question sanctioned. The `Some(&mut a)` never
    // outlives the `match`, so it doesn't escape; the capture binds the view and
    // writes through. Both the direct subject and the conditional form (`match if
    // c { Some(..) } else { None }`, the inline analogue of `Arena::get`).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut a = 5;
            match Some(&mut a) {          // direct scalar transient
                Some(let v) => { v = 99; }
                None => {}
            }
            print(a);                    // 99 — written through

            mut b = 10;
            let take = false;
            match if take { Some(&mut b) } else { None } {   // conditional
                Some(let v) => { v = 1; }
                None => { print("none"); }
            }
            print(b);                    // 10 — None branch, untouched
        }
        "#,
        "99\nnone\n10\n",
    );
}

#[test]
fn an_inline_aggregate_option_view_transient_writes_through() {
    // C5.2, aggregate flavor: the payload is a `&mut struct`, so the capture is
    // the value's own reference and `.field` write-through reaches the original.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        struct Node { value: i32 }
        fun main() {
            mut node = Node { value = 1 };
            match Some(&mut node) {
                Some(let v) => { v.value = 42; }
                None => {}
            }
            print(node.value);           // 42
        }
        "#,
        "42\n",
    );
}

#[test]
fn a_view_parameter_forwarded_into_an_inline_transient_writes_through() {
    // C5.2, forward flavor: a bare `&mut` parameter passed straight into the
    // inline constructor (`Some(p)`) — the capture aliases the same view, so the
    // write reaches the caller's value. Scalar (`(base, key)`) and aggregate.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        struct Node { value: i32 }
        fun bump_scalar(p: &mut i32) {
            match Some(p) { Some(let v) => { v += 1; } None => {} }
        }
        fun bump_field(p: &mut Node) {
            match Some(p) { Some(let v) => { v.value += 1; } None => {} }
        }
        fun main() {
            mut a = 41;
            bump_scalar(&mut a);
            print(a);              // 42

            mut n = Node { value = 41 };
            bump_field(&mut n);
            print(n.value);        // 42
        }
        "#,
        "42\n42\n",
    );
}

#[test]
fn a_forwarded_immutable_view_transient_rejects_a_write() {
    // C5.2 boundary: forwarding a `&` (read-only) view keeps its convention — a
    // write through the capture is still rejected.
    assert_fails(
        r#"
        import std::option::Option::{ self, Some, None };
        fun peek(p: &i32) {
            match Some(p) { Some(let v) => { v = 9; } None => {} }
        }
        fun main() { mut a = 5; peek(&a); }
        "#,
    );
}

#[test]
fn a_stored_inline_option_view_is_rejected() {
    // C5.2 boundary: the sanction is for the *transient* only. Binding the same
    // `Some(&mut a)` to a `let` stores the view in an enum payload that outlives
    // the statement — a real escape, still rejected.
    assert_fails(
        r#"
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut a = 5;
            let stored = Some(&mut a);
            match stored {
                Some(let v) => { v = 9; }
                None => {}
            }
        }
        "#,
    );
}

#[test]
fn transparent_references_reject_value_into_view_binding() {
    // R1: a view annotation (`&mut T`) cannot bind a value.
    assert_fails(
        r#"
        fun main() { mut a = 5; let v: &mut i32 = &mut a; let b: &mut i32 = *v; }
        "#,
    );
}

// --- C8: `Arena.get` migrated to the view-returning form --------------------
// `fun get(&self, handle): Option<&T> borrows self` (memory-management-rev-1
// §"A reusable arena in std"; spec §6.0/§6.7's table names this as current).
// The recognized wrapped-view leaf is `Some(&<T-place>)`, so std's `Slot` now
// stores `value: T` (not `Option<T>`) to expose that place; occupancy is
// generation-only, exactly as the proposal's own `get`/`remove` check.

#[test]
fn arena_get_returns_a_readable_view() {
    // The view reads into the arena: both a scalar field and a `List` field of
    // the live value are reachable through the `Some(let node)` capture — the
    // graph-walk shape a view-returning `get` exists for.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        struct Node { value: i32, edges: List<i32> }
        fun main() {
            mut arena: Arena<Node> = Arena::new();
            let h = arena.insert(Node { value = 7, edges = [1, 2] });
            match arena.get(h) {
                Some(let node) => {
                    print(node.value);           // 7 — field read through the view
                    mut total = 0;
                    for edge in node.edges { total = total + edge; }
                    print(total);                // 3 — list field walked through the view
                }
                None => { print(-1); }
            }
        }
        "#,
        "7\n3\n",
    );
}

#[test]
fn arena_get_on_a_stale_handle_is_none() {
    // Removal bumps the slot's generation, so the old handle no longer matches
    // and `get` returns `None`. A reused slot takes the bumped generation, so an
    // old handle to it stays stale; an untouched handle keeps reading.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        fun read(arena: Arena<i32>, handle: Handle<i32>): i32 {
            match arena.get(handle) { Some(let v) => *v, None => -1 }
        }
        fun main() {
            mut arena: Arena<i32> = Arena::new();
            let a = arena.insert(10);
            let b = arena.insert(20);
            arena.remove(b);
            print(read(arena, b));               // -1 — stale after removal
            let c = arena.insert(30);            // reuses b's slot at a new generation
            print(read(arena, c));               // 30
            print(read(arena, b));               // -1 — old handle stays stale
            print(read(arena, a));               // 10 — untouched
        }
        "#,
        "-1\n30\n-1\n10\n",
    );
}

#[test]
fn arena_get_on_a_data_arena_round_trips() {
    // A scalar/data arena's whole cycle is unchanged by the migration: insert,
    // read via `get`, overwrite via `set`, `remove` (owned `Option<T>`), `len`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut arena: Arena<i32> = Arena::new();
            let a = arena.insert(1);
            let b = arena.insert(2);
            print(arena.len());                  // 2
            arena.set(b, 99);
            match arena.get(b) { Some(let v) => print(*v), None => print(-1) }  // 99
            print(arena.remove(a).unwrap_or(-1)); // 1
            print(arena.len());                  // 1
        }
        "#,
        "2\n99\n1\n1\n",
    );
}

#[test]
fn arena_get_returns_a_view_not_a_copy() {
    // The distinguisher from the old copy-returning `get(): Option<T>`: the
    // `Some(let view)` capture is now a *view*, so storing it in a struct field
    // is a view escape. Under the old form `view` was an owned `Cell` and this
    // compiled — turning it into an error is exactly what the migration does.
    assert_fails_with(
        r#"
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        struct Cell { n: i32 }
        struct Keeper { held: Cell }
        fun main() {
            mut arena: Arena<Cell> = Arena::new();
            let h = arena.insert(Cell { n = 1 });
            match arena.get(h) {
                Some(let view) => { let k = Keeper { held = view }; }
                None => {}
            }
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn arena_mutation_under_a_live_get_view_is_rejected() {
    // C10 closed (rule-4 completion S3): a wrapped-view `match` capture anchors
    // at the arena, so a BUMPING mutation (`insert` — grows/reuses slots) inside
    // the arm fires E2. (`set` no longer invalidates — it is the stable table
    // row; the accept twin is `arena_set_under_a_live_get_view_is_accepted`.)
    assert_fails_with(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut arena: Arena<i32> = Arena::new();
            let h = arena.insert(10);
            match arena.get(h) {
                Some(let v) => { arena.insert(30); print(*v); }
                None => {}
            }
        }
        "#,
        "while a view into it is live",
    );
}

#[test]
fn arena_set_under_a_live_get_view_is_accepted() {
    // The C10+C6 showcase: the capture is anchored (C10), and `set` — an
    // in-place slot overwrite, the stable table row — does not bump (C6), so
    // holding the view across it is legal.
    assert_compiles(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut arena: Arena<i32> = Arena::new();
            let h = arena.insert(10);
            let h2 = arena.insert(11);
            match arena.get(h) {
                Some(let v) => { arena.set(h2, 20); print(*v); }
                None => {}
            }
        }
        "#,
    );
}

// --- C7: wire-blessed handles (`claims-and-epochs.md` §6) -------------------
// A handle is a NAME — durable identity plus the epoch to re-validate against —
// so it is the one alias that crosses the wire. `Handle<T>` now carries
// `[derive(Wire)]`; the `T` is phantom, so the payload is exactly the two
// integers, and a `[derive(Wire)]` type may carry a handle field.

#[test]
fn a_handle_round_trips_through_the_json_codec() {
    // The naming-layer idiom end to end: a handle issued by a server-side arena
    // encodes as `{index, generation}`, decodes back, and still resolves.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::json::{ encode_json, decode_json };
        import std::result::Result::{ self, Ok, Err };
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut arena: Arena<i32> = Arena::new();
            let handle = arena.insert(7);
            let text = encode_json(handle);
            print(text);
            let back: Result<Handle<i32>, str> = decode_json(text);
            match back {
                Ok(let name) => print(arena.get(name).unwrap_or(-1)),
                Err(let reason) => print(reason),
            }
        }
        "#,
        "{\"index\":0,\"generation\":0}\n7\n",
    );
}

#[test]
fn a_handle_round_trips_through_the_binary_codec() {
    // The same name over the binary codec — the visitor impls the derive emits
    // are codec-neutral, so both channels rebuild the same two integers.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::binary::{ encode_binary, decode_binary };
        import std::result::Result::{ self, Ok, Err };
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut arena: Arena<i32> = Arena::new();
            let a = arena.insert(5);
            let b = arena.insert(6);
            let back: Result<Handle<i32>, str> = decode_binary(encode_binary(b));
            match back {
                Ok(let name) => print(arena.get(name).unwrap_or(-1)),
                Err(let reason) => print(reason),
            }
            print(arena.get(a).unwrap_or(-1));
        }
        "#,
        "6\n5\n",
    );
}

#[test]
fn a_stale_handle_from_the_wire_resolves_to_none() {
    // The distributed staleness story: a client acting on an entity another
    // client deleted gets the SAME clean `None` as local code holding a stale
    // handle — no phantom write, one rule from a local `List` to an RPC
    // boundary.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::json::{ encode_json, decode_json };
        import std::result::Result::{ self, Ok, Err };
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut arena: Arena<i32> = Arena::new();
            let handle = arena.insert(7);
            let quoted = encode_json(handle);        // the client keeps the name
            arena.remove(handle);                    // someone else deletes it
            let back: Result<Handle<i32>, str> = decode_json(quoted);
            match back {
                Ok(let name) => print(arena.get(name).unwrap_or(-1)),  // -1
                Err(let reason) => print(reason),
            }
            // `set` through a stale name changes nothing, and reports it.
            match back {
                Ok(let name) => print(arena.set(name, 99)),            // false
                Err(let reason) => print(reason),
            }
        }
        "#,
        "-1\nfalse\n",
    );
}

#[test]
fn a_wire_type_may_carry_a_handle_field() {
    // The phantom-parameter case the derive had to tolerate: `Handle<T>` is an
    // APPLIED derived type, which the all-fields Wire check used to reject
    // outright ("which is not Wire"). The `T` never reaches the payload.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::json::{ encode_json, decode_json };
        import std::result::Result::{ self, Ok, Err };
        import std::option::Option::{ self, Some, None };
        [derive(Wire)]
        struct Rename { node: Handle<str>, title: str }
        fun main() {
            mut arena: Arena<str> = Arena::new();
            let handle = arena.insert("old");
            let command = Rename { node = handle, title = "new" };
            let text = encode_json(command);
            print(text);
            let back: Result<Rename, str> = decode_json(text);
            match back {
                Ok(let request) => {
                    arena.set(request.node, request.title);
                    print(arena.get(request.node).unwrap_or("gone"));
                }
                Err(let reason) => print(reason),
            }
        }
        "#,
        "{\"node\":{\"index\":0,\"generation\":0},\"title\":\"new\"}\nnew\n",
    );
}

#[test]
fn a_handle_names_an_entity_whose_type_is_not_itself_wire() {
    // A name is not the thing it names: `Handle<T>` is sendable whatever `T` is
    // — the point of the naming layer (the entity stays on the server). The
    // generic argument is deliberately unconstrained, which is sound only
    // because a derived type's parameters are necessarily phantom
    // (`a_wire_type_with_a_parameter_typed_field_is_rejected` is the other half).
    assert_compiles(
        r#"
        import std::arena::{ Arena, Handle };
        struct Session { socket: |str| void }
        [derive(Wire)]
        struct Close { target: Handle<Session> }
        fun main() {
            mut sessions: Arena<Session> = Arena::new();
            let handle = sessions.insert(Session { socket = |line| {} });
            let close = Close { target = handle };
        }
        "#,
    );
}

#[test]
fn a_wire_type_with_a_parameter_typed_field_is_rejected() {
    // The guard behind C7's unconstrained generic arguments: a `[derive(Wire)]`
    // type whose field is typed by a PARAMETER is rejected at its own
    // declaration (the derive emits no generic impls), so no derived type can
    // put a generic argument on the wire. If generic Wire derives ever land,
    // `is_wire_type` must start checking the arguments.
    assert_fails_with(
        r#"
        [derive(Wire)]
        struct Pair<T> { value: T, count: i32 }
        "#,
        "which is not Wire",
    );
}

#[test]
fn a_branded_arena_rejects_a_foreign_handle() {
    // `claims-and-epochs.md` §6's capability note: per-session arenas are the
    // blessed default, and anything cross-tenant adds a per-arena random brand
    // so a handle from one arena names nothing in another. Without a brand, an
    // equal-index/equal-generation handle from a DIFFERENT arena resolves —
    // which is why the per-session scoping stays the rule and the brand is the
    // belt to it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        fun main() {
            // The UNBRANDED control first — it is what makes this pin
            // discriminate: two plain arenas number from 0, so the foreign
            // handle resolves to the other arena's slot of the same index.
            mut plain: Arena<i32> = Arena::new();
            mut plain_other: Arena<i32> = Arena::new();
            let loose = plain.insert(7);
            plain_other.insert(9);
            print(plain_other.get(loose).unwrap_or(-1));   // 9 — the confusion

            mut mine: Arena<i32> = Arena::branded();
            mut theirs: Arena<i32> = Arena::branded();
            let handle = mine.insert(7);
            theirs.insert(9);
            print(theirs.get(handle).unwrap_or(-1));   // -1 — a foreign name
            print(mine.get(handle).unwrap_or(-1));     // 7
        }
        "#,
        "9\n-1\n7\n",
    );
}

#[test]
fn a_branded_arenas_generational_cycle_is_unchanged() {
    // Branding only moves where the counters START, so every generational rule
    // holds above the brand: removal bumps the slot, the old handle goes stale,
    // and a reused slot issues a fresh handle that reads. (The plain-arena twin
    // is `arena_get_on_a_stale_handle_is_none`.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::arena::{ Arena, Handle };
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut arena: Arena<i32> = Arena::branded();
            let handle = arena.insert(7);
            print(arena.get(handle).unwrap_or(-1));     // 7
            arena.remove(handle);
            print(arena.get(handle).unwrap_or(-1));     // -1 — stale
            let reused = arena.insert(30);              // reuses the slot
            print(arena.get(reused).unwrap_or(-1));     // 30
            print(arena.get(handle).unwrap_or(-1));     // -1 — stays stale
            print(arena.set(handle, 99));               // false — no phantom write
            print(arena.len());                         // 1
        }
        "#,
        "7\n-1\n30\n-1\nfalse\n1\n",
    );
}

// --- rule4-completion S1: the `borrows` root-set (inference only) -----------
// `Function.borrows` records *which* parameter positions a returned view
// projects (receiver = position 0), inferred and chained. Inference-only: no
// enforcement changed, the corpus stays byte-identical. These pin the behavior
// each root-set drives; the projected positions themselves surface in the
// language-server hover tests (`borrows self`, `borrows a, b`, `borrows b`).

#[test]
fn direct_projection_borrows_the_receiver() {
    // A `&mut self` method returning `&mut self.field` projects the receiver
    // (position 0): the write through the projection reaches the receiver, and a
    // binding of the call is a writable view. The inferred twin of `borrows.vl`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Wrapper { value: i32 }
        impl Wrapper { fun slot(&mut self): &mut i32 { &mut self.value } }
        fun main() {
            mut w = Wrapper { value = 1 };
            w.slot() = 10;
            print(w.value);          // 10 — written through the projection
            let v = w.slot();
            v = 25;
            print(w.value);          // 25 — written through the bound view
        }
        "#,
        "10\n25\n",
    );
}

#[test]
fn chained_projection_maps_through_a_borrows_call() {
    // A return leaf that is itself a borrows-call: `outer` returns `self.inner()`
    // where `inner` borrows self, so the callee's {0} maps back through the
    // receiver to `outer`'s {0}. Before the root-set this call-tail was not
    // recognized as a view (it miscompiled); the chain now lowers it correctly.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Wrapper { value: i32 }
        impl Wrapper {
            fun inner(&mut self): &mut i32 borrows self { &mut self.value }
            fun outer(&mut self): &mut i32 { self.inner() }
        }
        fun main() {
            mut w = Wrapper { value = 1 };
            w.outer() = 42;
            print(w.value);          // 42
        }
        "#,
        "42\n",
    );
}

#[test]
fn chained_projection_maps_a_non_receiver_argument() {
    // The chain maps the callee's *position* through the call's arguments: a free
    // `pick(a, b)` returning `grow(b)` — where `grow` borrows its position-0
    // parameter — projects `b`, the caller's position 1, not `a`. Only `q` (bound
    // to `b`) is written; `p` is untouched, proving the mapping is positional.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun grow(x: &mut i32): &mut i32 borrows x { x }
        fun pick(a: &mut i32, b: &mut i32): &mut i32 { grow(b) }
        fun main() {
            mut p = 1;
            mut q = 2;
            pick(&mut p, &mut q) = 9;
            print(p);                // 1 — untouched
            print(q);                // 9 — projected through b
        }
        "#,
        "1\n9\n",
    );
}

#[test]
fn multi_parameter_projection_unions_branch_positions() {
    // An `if` returning a wrapped view of a *different* parameter per leg unions
    // their positions → {0, 1}: each branch's projection writes through to the
    // chosen parameter. The every-leaf-agrees rule still holds (both legs `&mut`,
    // both aggregate) — a recognized wrapped view, not an escape.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        struct Box { x: i32 }
        fun pick(a: &mut Box, b: &mut Box, first: bool): Option<&mut i32> {
            if first { Some(&mut a.x) } else { Some(&mut b.x) }
        }
        fun main() {
            mut p = Box { x = 1 };
            mut q = Box { x = 2 };
            match pick(&mut p, &mut q, true) { Some(let v) => { v = 90; } None => {} }
            match pick(&mut p, &mut q, false) { Some(let v) => { v = 91; } None => {} }
            print(p.x);              // 90 — first branch projected a
            print(q.x);              // 91 — second branch projected b
        }
        "#,
        "90\n91\n",
    );
}

#[test]
fn a_wrapped_view_return_projects_its_parameter() {
    // The wrapped `Option<&mut T>` shape records the projected position exactly
    // like a bare view return: un-annotated, `slot` borrows self (position 0),
    // and the captured view writes through. (The `transparent-references.vl`
    // `Cell::slot` shape — the root-set now records it without changing codegen.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        struct Cell { value: i32 }
        impl Cell { fun slot(&mut self): Option<&mut i32> { Some(&mut self.value) } }
        fun main() {
            mut cell = Cell { value = 1 };
            match cell.slot() { Some(let v) => { v = 7; } None => {} }
            print(cell.value);       // 7
        }
        "#,
        "7\n",
    );
}

#[test]
fn an_explicit_borrows_clause_agrees_with_inference() {
    // `borrows self` names position 0; inference of the same body also yields
    // {0} — they agree (the union is idempotent, no check contradicts). The
    // annotated form compiles and writes through identically to the inferred one.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Wrapper { value: i32 }
        impl Wrapper { fun slot(&mut self): &mut i32 borrows self { &mut self.value } }
        fun main() {
            mut w = Wrapper { value = 3 };
            w.slot() = 8;
            print(w.value);          // 8
        }
        "#,
        "8\n",
    );
}

#[test]
fn a_returned_view_of_a_local_is_still_rejected() {
    // The escape boundary is unchanged: a view of a *local* (not a parameter)
    // projects no position, so the root-set stays empty and the view escapes —
    // rejected exactly as before the root-set. (S1 records positions; it does
    // not relax enforcement.)
    assert_fails_with(
        r#"
        fun leak(): &mut i32 { mut local = 1; &mut local }
        fun main() { let v = leak(); }
        "#,
        "a view cannot escape its scope",
    );
}

// --- A1: `Shared::write(): &mut T borrows self` -----------------------------

#[test]
fn shared_write_view_rebinds_and_mutates_through_handles() {
    // Writing a whole value through the view rebinds the cell's slot, so every
    // handle (a clone) sees it; a method call mutates in place. The rebind must
    // NOT merge — the old aggregate-view `Object.assign` path would have left a
    // stale tail (len 3 then 4 instead of 1 then 2).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::shared::Shared;
        fun main() {
            let a: Shared<List<i32>> = Shared::new([1, 2, 3]);
            let b = a.clone();
            a.write() = [9];
            print(b.read().len());
            a.write().push(8);
            print(b.read().len());
        }
        "#,
        "1\n2\n",
    );
}

#[test]
fn own_parameter_is_a_mutable_copy() {
    // `own x: T` consumes a copy the callee may mutate freely — reassign a scalar,
    // or rebind an aggregate — without affecting the caller (an aggregate is
    // cloned at the call site). Reassigning an `own` parameter used to be rejected
    // ("cannot assign to this expression"); it is now allowed.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun bump(own x: i32): i32 { x += 1; x }
        fun grow(own xs: List<i32>): i32 { xs = [7, 8, 9, 10]; xs.len() }
        fun main() {
            mut a = 10;
            print(bump(a)); // 11
            print(a);       // 10 — caller untouched
            mut list = [1, 2];
            print(grow(list)); // 4
            print(list.len()); // 2 — caller untouched
        }
        "#,
        "11\n10\n4\n2\n",
    );
}

// --- H9: `mut` parameters (proposal/mut-parameters.md) ---------------------
// `fun f(mut x: T) { body }` ≡ `fun f(x': T) { mut x = x'; body }` — binder
// mutability of the callee's by-value copy, exclusive with conventions,
// never part of the signature.

#[test]
fn a_mut_parameter_rebinds_its_copy() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun bump(mut x: i32): i32 { x = x + 1; x }
        fun main() { print(bump(1)); }
        "#,
        "2\n",
    );
}

#[test]
fn a_mut_parameter_is_invisible_to_the_caller() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun grow(mut xs: List<i32>): i32 { xs.push(9); xs.len() }
        fun main() {
            mut list = [1, 2];
            print(grow(list));  // 3 — the callee's copy grew
            print(list.len());  // 2 — the caller's value untouched
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_plain_parameter_beside_a_mut_one_still_rejects_writes() {
    // The mixed list pins two things: `mut` is per-parameter, and the plain
    // parameter's rejection now offers BOTH spellings (copy vs caller).
    assert_fails_with(
        r#"
        fun f(mut a: i32, b: i32) { a = 1; b = 2; }
        fun main() { f(1, 2); }
        "#,
        "declare it `mut b` to mutate this function's copy, or `&mut b` to mutate the caller's value",
    );
}

#[test]
fn a_mut_parameter_takes_field_writes() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32 }
        fun place(mut p: Point): i32 { p.x = 42; p.x }
        fun main() { print(place(Point { x = 0 })); }
        "#,
        "42\n",
    );
}

#[test]
fn a_closure_mut_parameter_works_unannotated() {
    // The field case that filed H9, verbatim: mutating a `Signal<List<T>>`
    // via `set_with(|mut list| { list.push(..); list })`. The closure's
    // parameter type lands from `set_with`'s declared signature.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::reactive::Signal;
        fun main() {
            mut seed = [1, 2];
            let numbers = Signal::new(seed);
            numbers.set_with(|mut list| {
                list.push(5);
                list
            });
            print(numbers.get().len());
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_closure_mut_parameter_types_from_a_declared_closure_argument() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun apply(xs: List<i32>, grow: |List<i32>| i32): i32 { grow(xs) }
        fun main() {
            mut list = [1, 2];
            print(apply(list, |mut xs| {
                xs.push(5);
                xs.len()
            }));
            print(list.len());
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_closure_mut_parameter_works_annotated() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let bump = |mut v: i32| { v = v + 1; v };
            print(bump(1));
        }
        "#,
        "2\n",
    );
}

#[test]
fn mut_self_is_the_builder_idiom() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32 }
        impl Point {
            fun with_x(mut self, value: i32): Point { self.x = value; self }
        }
        fun main() {
            let original = Point { x = 0 };
            let moved = original.with_x(9);
            print(moved.x);
            print(original.x); // the receiver copy mutated; the original didn't
        }
        "#,
        "9\n0\n",
    );
}

#[test]
fn a_mut_parameter_roots_a_writable_view() {
    // `&mut x` of a `mut` parameter's copy is fine (readonly_root clears);
    // the view writes land in the copy, still invisible to the caller.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun poke(mut x: i32): i32 { let v = &mut x; v = 5; x }
        fun main() { print(poke(1)); }
        "#,
        "5\n",
    );
}

#[test]
fn a_mut_parameter_feeds_a_ref_mut_argument() {
    // A `mut` parameter is a mutable place, so passing `&mut` of it to a
    // writable-view parameter passes check_mutable_arguments.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun bump(c: &mut i32) { c += 1; }
        fun outer(mut x: i32): i32 { bump(&mut x); x }
        fun main() { print(outer(1)); }
        "#,
        "2\n",
    );
}

#[test]
fn mut_does_not_combine_with_a_convention() {
    assert_fails_with(
        r#"
        fun f(mut own x: i32) {}
        fun main() { f(1); }
        "#,
        "cannot combine with `own` or a view",
    );
}

#[test]
fn mut_does_not_combine_with_an_inferred_view_convention() {
    // `mut x: &mut i32` — no prefix, but the type makes the convention RefMut.
    assert_fails_with(
        r#"
        fun f(mut x: &mut i32) {}
        fun main() {}
        "#,
        "cannot combine with `own` or a view",
    );
}

#[test]
fn mut_needs_a_name_binder_not_a_destructure() {
    assert_fails_with(
        r#"
        fun f(mut (a, b): (i32, i32)) {}
        fun main() {}
        "#,
        "applies to a plain name",
    );
}

#[test]
fn an_external_fun_rejects_mut_parameters() {
    assert_fails_with(
        r#"
        external fun host(mut x: i32);
        fun main() {}
        "#,
        "an `external fun` has no body",
    );
}

#[test]
fn a_mut_parameter_never_takes_a_resource() {
    // R1: a resource never copies, and a `mut` parameter IS a copy — the
    // rejection steers to `own` (transfer), the sanctioned resource intake.
    assert_fails_with(
        r#"
        resource struct Conn { id: i32 }
        impl Conn { fun close(own self) {} }
        fun misuse(mut c: Conn) {}
        fun main() {}
        "#,
        "a resource never copies",
    );
}

#[test]
fn a_mut_destructure_capture_does_not_alias_its_source() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = ([1, 2], 3);
            mut (xs, n) = pair;
            xs.push(9);
            print(xs.len());
            print(pair.0.len());
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn an_immutable_capture_is_isolated_from_source_mutation() {
    // B53, the read direction: after `let (xs, _) = pair`, growing `pair.0`
    // must not show through `xs` — the capture copied (subject roots a
    // `mut` binding, so the share elision does not apply).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = ([1, 2], 3);
            let (xs, n) = pair;
            pair.0.push(9);
            print(xs.len());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_mut_match_capture_does_not_alias_the_subject() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = ([1, 2], 3);
            match pair {
                (mut xs, let n) => {
                    xs.push(9);
                    print(xs.len());
                    print(pair.0.len());
                }
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_returned_capture_does_not_leak_an_alias() {
    // B53's seam case, the `unwrap` leak: a capture that IS the leg's value
    // rides the return out of the callee — it must be a copy, or the
    // caller's `mut got` mutates the option's payload through the alias
    // (the "calls own their result" assumption every binding-copy elision
    // rests on).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let held: Option<List<i32>> = Some([1, 2]);
            mut got = held.unwrap();
            got.push(9);
            print(got.len());
            match held {
                Some(let inner) => print(inner.len()),
                None => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_nested_variant_capture_does_not_alias() {
    // The capture sits two pattern levels deep (variant payload inside a
    // tuple) — collection recurses the whole tree, not just top level.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut wrapped = (Some([1, 2]), 3);
            match wrapped {
                (Some(mut xs), let n) => {
                    xs.push(9);
                    print(xs.len());
                }
                (None, _) => print(0),
            }
            match wrapped {
                (Some(let inner), _) => print(inner.len()),
                (None, _) => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn an_is_capture_does_not_alias_the_subject() {
    // B53 finding 1: `is` captures compile through the ALIAS path
    // (`compile_is_pattern` records an accessor into the subject and
    // substitutes it at every reference), which the first pass never taught
    // to copy — so this printed 3, the source's growth showing through.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = ([1, 2], 3);
            if pair is (let xs, let n) {
                pair.0.push(9);
                print(xs.len());
            }
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_mut_is_capture_does_not_write_back_to_the_subject() {
    // The write direction of the same hole: `mut v` aliased the option's
    // payload, so growing it grew what the option still holds (3/3).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut held: Option<List<i32>> = Some([1, 2]);
            if held is Some(mut v) {
                v.push(9);
                print(v.len());
            }
            match held {
                Some(let inner) => print(inner.len()),
                None => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_guarded_match_capture_does_not_alias_the_subject() {
    // A GUARD moves the leg onto the alias path too (the guard reads the
    // captures, so they cannot be `const`s inside the body) — the same
    // program as `a_mut_match_capture_does_not_alias_the_subject` printed
    // 3/3 with `if n > 0` added.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = ([1, 2], 3);
            match pair {
                (mut xs, let n) if n > 0 => {
                    xs.push(9);
                    print(xs.len());
                    print(pair.0.len());
                }
                _ => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_rejecting_guard_leaves_the_subject_untouched() {
    // The copy is made on ENTRY to the leg body, not when the pattern
    // matches: the first leg's guard rejects, so it has copied nothing and
    // consumed nothing and the SECOND guarded leg finds the subject exactly
    // as it was — and that leg's own capture is still a copy (3/3 before).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = ([1, 2], 3);
            match pair {
                (mut xs, let n) if n > 100 => {
                    xs.push(9);
                    print(0);
                }
                (mut xs, let n) if n > 0 => {
                    xs.push(7);
                    print(xs.len());
                    print(pair.0.len());
                }
                _ => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_braced_leg_capture_does_not_leak_an_alias() {
    // B53 finding 4: the value-seam scan only saw seams that were
    // syntactically a place, so BRACING the leg body — `Some(let inner) => {
    // inner }` — hid the seam and restored the `unwrap` leak (3/3). The scan
    // now looks through the forms that forward a value.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun take_it(held: Option<List<i32>>, fallback: List<i32>): List<i32> {
            match held {
                Some(let inner) => { inner }
                None => fallback,
            }
        }
        fun main() {
            let held: Option<List<i32>> = Some([1, 2]);
            mut got = take_it(held, List::new());
            got.push(9);
            print(got.len());
            match held {
                Some(let inner) => print(inner.len()),
                None => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_conditionally_returned_capture_does_not_leak_an_alias() {
    // The same hole through an `if` tail: neither `a` nor `b` is the
    // function's tail EXPRESSION, so neither rooted a seam and both shared
    // the caller's tuple (3/3).
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun pick(pair: (List<i32>, List<i32>), first: bool): List<i32> {
            let (a, b) = pair;
            if first { a } else { b }
        }
        fun main() {
            let pair = ([1, 2], [5]);
            mut got = pick(pair, true);
            got.push(9);
            print(got.len());
            print(pair.0.len());
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_shared_capture_is_not_an_elidable_move_source() {
    // B53 finding 3: the share elision and rule 2's move elision are each
    // sound alone and composed unsoundly — `xs` shared `pair.0` (immutable
    // capture, immutable subject, no seam) and was then read exactly once,
    // which made it an elidable-copy source, handing the shared storage to a
    // `mut` binding. `const xs = $a[0]; let ys = xs;` with no copy anywhere:
    // this printed 3. Only an OWNER may donate.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pair = ([1, 2], 3);
            let (xs, n) = pair;
            mut ys = xs;
            ys.push(9);
            print(pair.0.len());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_mut_capture_from_an_immutable_subject_copies() {
    // The share elision's other guard, on an IMMUTABLE subject (where the
    // elision is otherwise live — the sibling pins all use `mut pair`, which
    // disables it before this configuration is reached): a `mut` capture is
    // never shareable, because it can write.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let pair = ([1, 2], 3);
            mut (xs, n) = pair;
            xs.push(9);
            print(xs.len());
            print(pair.0.len());
        }
        "#,
        "3\n2\n",
    );
}

// --- B63(a): the share elision's predicate is SEMANTIC, not the diagnostic one -
//
// `readonly_root` answers "what `declare it …` advice applies", and it owes
// `None` for an `own` parameter because the advice would be `mut own x`, a parse
// error. The elision's question is different — "can this place change" — and an
// `own` parameter that nothing writes cannot. B60 reused the diagnostic helper
// for the semantic decision, which cost every `own self` combinator its share on
// data paths (`affine-moves.md` §5/§6); `share_subject_is_stable` splits them.

#[test]
fn an_own_parameter_capture_shares_when_nothing_writes_it() {
    // The elision itself, which no runtime output can see: the capture's slot
    // read emits bare. The one program-wide `__clone` site is this capture, so
    // asserting the helper is absent entirely is exact. (`Option::map`'s
    // monomorphized body in `vilan/test/closure-param-inference.js` is the
    // same elision seen in bytes — it regained its pre-B60 form here.)
    let source = r#"
        import std::print;
        fun peek(own pair: (List<i32>, i32)): i32 {
            let (first, second) = pair;
            first.len()
        }
        fun main() { print(peek(([ 1, 2 ], 3))); }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "the `own`-parameter capture still copies:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "2\n");
}

#[test]
fn an_own_parameter_capture_copies_when_a_method_writes_it() {
    // The soundness boundary the split creates, and why the predicate is a
    // WRITE SET rather than a blanket `own` admission: an `own` parameter is
    // freely writable (`h.n = 5` inside `fun f(own h: Holder)` compiles), so a
    // body that mutates it can observe the alias. `push` takes `&mut self`, so
    // the receiver's root joins the write set and the capture copies — 1, not 2.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun observe(own pair: (List<i32>, i32)): i32 {
            let (first, second) = pair;
            pair.0.push(7);
            first.len()
        }
        fun main() { print(observe(([ 1 ], 2))); }
        "#,
        "1\n",
    );
}

#[test]
fn an_own_parameter_capture_copies_when_an_assignment_writes_it() {
    // The write set's other source, on its own pin because the two are found
    // by different arms: a plain field assignment through the `own` parameter.
    // The capture must not see `[ 9, 9 ]` — 1, not 2.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { xs: List<i32> }
        fun observe(own pair: (Holder, i32)): i32 {
            let (first, second) = pair;
            pair.0.xs = [ 9, 9 ];
            first.xs.len()
        }
        fun main() { print(observe((Holder { xs = [ 1 ] }, 2))); }
        "#,
        "1\n",
    );
}

#[test]
fn a_generic_capture_moves_a_resource_instantiation() {
    // B53 finding 2, R11 (`docs/spec/memory.md`): a capture typed by a bare
    // generic parameter copied conservatively in EVERY monomorphization, so
    // `Option::unwrap`'s `Some(let x) => x` deep-copied a resource — two
    // owners with divergent state, where the spec names `unwrap(own self): T`
    // as the case that must pass with no copies.
    //
    // B60 then made the call a MOVE, so the source cannot be read afterwards
    // to compare the two (that is this test's rejection half, below); the
    // copy-vs-move evidence now runs through the destructor instead — a copy
    // would destroy two values with divergent fields, and
    // `a_moved_resource_instantiation_destroys_one_value` pins the single
    // `drop a n=7` that proves the caller holds THE resource, not a twin.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Res {
            n: i32,
        }
        fun main() {
            mut o: Option<Res> = Some(Res { n = 1 });
            mut r = o.unwrap();
            r.n = 7;
            print(r.n);
        }
        "#,
        "7\n",
    );
}

#[test]
fn a_moved_resource_instantiation_destroys_one_value() {
    // The same fix seen through the destructor. The copy made TWO resources
    // and ran `drop` on each with divergent fields (`n=7` then `n=1`); the
    // move makes one, so both runs report the same value.
    //
    // It used to drop TWICE (`...done\ndrop a n=7`) — B60: `unwrap` took a
    // LOANED `self`, so the call marked no move and `o`'s scope-end teardown
    // still fired over the payload the caller now owned. `unwrap(own self)`
    // plus the loan-consumption rule closes it: the call is a move, so `o` is
    // not owned at scope end and the value is destroyed exactly once.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        import std::drop::{ Drop, drop };
        resource struct Res {
            tag: str,
            n: i32,
        }
        impl Res with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag} n={self.n}");
            }
        }
        fun main() {
            mut o: Option<Res> = Some(Res { tag = "a", n = 1 });
            mut r = o.unwrap();
            r.n = 7;
            print(i"r.n={r.n}");
            drop(r);
            print("done");
        }
        "#,
        "r.n=7\ndrop a n=7\ndone\n",
    );
}

#[test]
fn a_generic_aggregate_capture_moves_a_resource_instantiation() {
    // The same decision one level up: the capture is typed `Wrap<T>`, an
    // aggregate whatever `T` is, so it is not a bare generic — but `Wrap<Res>`
    // IS a resource, and the copy made two (7/1). The analyzer records which
    // constraints can turn the capture into a resource; the transformer asks
    // what this instance bound them to.
    //
    // `first_of` takes `own pair` (B60: a body may only consume what it owns —
    // it used to take the loan `pair: (Wrap<T>, i32)` and move a piece of it
    // out, which is what let `main` read `pair.0` afterwards and see the SAME
    // value through two owners). With the move enforced, the copy-vs-move
    // question is answered through the destructor: one `drop n=7` is the
    // single mutated resource; a copy would add a second at `n=1`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res {
            n: i32,
        }
        impl Res with Drop {
            fun drop(&mut self) {
                print(i"drop n={self.n}");
            }
        }
        struct Wrap<T> {
            value: T,
        }
        fun first_of<T>(own pair: (Wrap<T>, i32)): Wrap<T> {
            let (a, b) = pair;
            a
        }
        fun main() {
            let pair = (Wrap { value = Res { n = 1 } }, 5);
            mut w = first_of(pair);
            w.value.n = 7;
            print(w.value.n);
        }
        "#,
        "7\ndrop n=7\n",
    );
}

#[test]
fn a_generic_aggregate_capture_copies_a_data_instantiation() {
    // The other half of the same gate: `Wrap<List<i32>>` is no resource, so
    // the same capture in the same function still copies. The carve-out is for
    // resources only — it is not a licence to alias.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Wrap<T> {
            value: T,
        }
        fun first_of<T>(pair: (Wrap<T>, i32)): Wrap<T> {
            let (a, b) = pair;
            a
        }
        fun main() {
            let pair = (Wrap { value = [1, 2] }, 5);
            mut w = first_of(pair);
            w.value.push(9);
            print(w.value.len());
            print(pair.0.value.len());
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_mut_array_binder_in_a_match_stamps_its_elements() {
    // B53 finding 5: `mut` at a binder applies to every binding under it, and
    // the match/`is` grammar recursed tuples but not arrays — so `mut [a, b]`
    // in a match bound `a`/`b` IMMUTABLE ("cannot mutate immutable 'a'") while
    // the identical `mut [a, b] = arr` bound them mutable. One keyword, two
    // meanings.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let arr: [List<i32>; 2] = [[1, 2], [3]];
            match arr {
                mut [a, b] => {
                    a.push(9);
                    print(a.len());
                    print(arr[0].len());
                }
                _ => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_mut_array_binder_in_an_is_test_stamps_its_elements() {
    // The `is` half of the same grammar arm.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let arr: [List<i32>; 2] = [[1, 2], [3]];
            if arr is mut [a, b] {
                a.push(9);
                print(a.len());
                print(arr[0].len());
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_guard_that_needs_a_temporary_emits_it() {
    // B59: a guard whose expression needs hoisted statements (an `is` test, a
    // `?` lift, a nested `match`) used to drop them — an else-if chain has no
    // statement slot before a leg's condition — and the emitted condition
    // referenced a temporary that was never declared. Such a leg is now emitted
    // with its own slot, and the copies its captures owe are declared ahead of
    // the guard, so the guard's `pop` takes from the copy, not the subject.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut pair = ([1, 2], 3);
            match pair {
                (mut xs, let n) if xs.pop() is Some(_) => {
                    print(xs.len());
                    print(pair.0.len());
                }
                _ => print(0),
            }
        }
        "#,
        "1\n2\n",
    );
}

// ---------------------------------------------------------------------------
// B81 (rule 1 through a VIEWED subject). B53 made the alias path COPY what it
// captures; it left the alias path READING late. Each capture stays an
// accessor into the subject temp, re-read at every reference, which is a
// faithful snapshot only while the subject can change by REBINDING — through a
// `&mut` view it changes IN PLACE (that is how the write reaches the caller),
// so every deferred read in the leg saw post-write state. Captures of a
// writable-view subject are now read once, at the match. See
// proposal/capture-clones.md §6.
// ---------------------------------------------------------------------------

#[test]
fn an_is_capture_from_a_mut_self_subject_reads_the_prematch_value() {
    // B81's filed repro. `at` is an `i32`, so it owes no copy and kept its
    // accessor `$a[2]`; `self = Feed::Ready(..)` lowers to a write in place
    // (`__replace(self, ..)`), mutating the very array `$a` aliases, so
    // `items[at]` indexed with the INCREMENTED `at`. Printed "b\nc" for two
    // steps over ["a","b","c"].
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        impl Feed {
            fun step(&mut self): Option<str> {
                if self is Feed::Ready(let items, let at) {
                    self = Feed::Ready(items, at + 1);
                    Some(items[at])
                } else {
                    None
                }
            }
        }
        fun main() {
            mut feed = Feed::Ready(["a", "b", "c"], 0);
            print(feed.step().unwrap());
            print(feed.step().unwrap());
        }
        "#,
        "a\nb\n",
    );
}

#[test]
fn an_is_capture_from_a_mut_parameter_subject_is_unchanged() {
    // The twin that was always right, pinned so the fix cannot move it: a
    // by-value `mut` parameter IS the callee's own copy (H9), so the subject
    // rebinds and each call re-reads the caller's untouched value. Not a view,
    // so no capture materializes and the emitted shape is byte-for-byte what
    // B53 shipped.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        fun step(mut feed: Feed): Option<str> {
            if feed is Feed::Ready(let items, let at) {
                feed = Feed::Ready(items, at + 1);
                Some(items[at])
            } else {
                None
            }
        }
        fun main() {
            mut feed = Feed::Ready(["a", "b", "c"], 0);
            print(step(feed).unwrap());
            print(step(feed).unwrap());
        }
        "#,
        "a\na\n",
    );
}

#[test]
fn an_is_capture_from_a_mut_view_parameter_reads_the_prematch_value() {
    // The same hole reached through an ordinary `&mut` parameter rather than
    // `&mut self` — the predicate is the parameter's CONVENTION, not the
    // receiver position. Printed "b\nc".
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        fun step(feed: &mut Feed): str {
            if feed is Feed::Ready(let items, let at) {
                feed = Feed::Ready(items, at + 1);
                items[at]
            } else {
                "-"
            }
        }
        fun main() {
            mut feed = Feed::Ready(["a", "b", "c"], 0);
            print(step(&mut feed));
            print(step(&mut feed));
        }
        "#,
        "a\nb\n",
    );
}

#[test]
fn an_is_capture_from_a_dereferenced_view_local_copies_and_reads_early() {
    // The second seam, and the worse one: `is_place_expr` excludes
    // `Expr::Dereference`, so a `*view` subject collected NO capture
    // candidates at all — B53's copy rule was missing wholesale for that
    // spelling, and even the AGGREGATE capture aliased (`__at($a[1], $a[2])`,
    // no `__clone` anywhere). Both halves are pinned here: `items` must be a
    // copy and `at` must be the pre-match index. Printed "a" only after both
    // fixes; before, "b".
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        fun main() {
            mut feed = Feed::Ready(["a", "b", "c"], 0);
            let view = &mut feed;
            if *view is Feed::Ready(let items, let at) {
                view = Feed::Ready(items, at + 1);
                print(items[at]);
            }
        }
        "#,
        "a\n",
    );
}

#[test]
fn a_destructure_of_a_dereferenced_view_copies_its_captures() {
    // The deref seam is not confined to the alias path — the capture pass
    // gates `Expr::Destructure` on the same predicate, so `let (xs, n) =
    // *view` collected no candidates either and B53's copy never fired. This
    // one has nothing to do with reading late: `xs` simply aliased the
    // subject's element, and growing it through the view grew the capture.
    // Printed 3.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = ([1, 2], 3);
            let view = &mut pair;
            let (xs, n) = *view;
            view.0.push(9);
            print(xs.len());
            print(n);
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_guarded_leg_capture_from_a_viewed_subject_reads_the_prematch_value() {
    // A guard puts the leg on the alias path too, so it carries the same hole
    // — and it is the ordering-sensitive one: `materialize_captures` runs
    // BEFORE the guard is walked and re-points the alias table, so a guard
    // that reads a materialized capture forces B59's prelude shape or the
    // emitted guard names a binding that has not been declared yet. Printed
    // "b\nc".
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        impl Feed {
            fun step(&mut self): str {
                match self {
                    Feed::Ready(let items, let at) if at >= 0 => {
                        self = Feed::Ready(items, at + 1);
                        items[at]
                    }
                    _ => "-",
                }
            }
        }
        fun main() {
            mut feed = Feed::Ready(["a", "b", "c"], 0);
            print(feed.step());
            print(feed.step());
        }
        "#,
        "a\nb\n",
    );
}

#[test]
fn an_unguarded_match_leg_on_a_viewed_subject_was_already_right() {
    // The negative half of the diagnosis, pinned: an UNGUARDED leg compiles
    // through `compile_pattern`, which declares every capture as a real
    // `const` at leg entry — it never reads late, so it never had the bug.
    // That asymmetry is what identified the alias path as the seam rather
    // than the view.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        impl Feed {
            fun step(&mut self): str {
                match self {
                    Feed::Ready(let items, let at) => {
                        self = Feed::Ready(items, at + 1);
                        items[at]
                    }
                    Feed::Done => "-",
                }
            }
        }
        fun main() {
            mut feed = Feed::Ready(["a", "b", "c"], 0);
            print(feed.step());
            print(feed.step());
        }
        "#,
        "a\nb\n",
    );
}

#[test]
fn both_capture_shapes_survive_an_in_place_write_through_the_view() {
    // The two payload shapes in one leg, and the sharper form of the write:
    // not a whole-subject reassignment but COMPONENT writes through the view
    // (`pair.0.push`, `pair.1 = 9`), which mutate the subject's storage
    // without going near the subject binding.
    //
    // `items` is B53's business — an aggregate capture COPIES, so growing the
    // source to 4 leaves it at 3. `at` is B81's — a scalar owes no copy, so it
    // kept an accessor and read the 9. One pin, red either way: 12 without the
    // materialization, 4 without the copy.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun grow(pair: &mut (List<str>, i32)): i32 {
            if pair is (let items, let at) {
                pair.0.push("d");
                pair.1 = 9;
                items.len() + at
            } else {
                -1
            }
        }
        fun main() {
            mut pair = (["a", "b", "c"], 0);
            print(grow(&mut pair));
            print(pair.0.len());
        }
        "#,
        "3\n4\n",
    );
}

#[test]
fn a_nested_capture_from_a_viewed_subject_reads_the_prematch_value() {
    // Every capture in the tree, not just the top level: the inner tuple's
    // `xs`/`k` and the outer `at` all read through the same subject temp.
    // 2 + 3 + 4; post-write it was 4 + 7 + 5.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Pair {
            Two((List<i32>, i32), i32),
            Neither,
        }
        impl Pair {
            fun step(&mut self): i32 {
                if self is Pair::Two((let xs, let k), let at) {
                    self = Pair::Two(([9, 9, 9, 9], 7), 5);
                    xs.len() + k + at
                } else {
                    -1
                }
            }
        }
        fun main() {
            mut pair = Pair::Two(([1, 2], 3), 4);
            print(pair.step());
        }
        "#,
        "9\n",
    );
}

#[test]
fn a_viewed_capture_read_before_and_after_the_write_agrees() {
    // Both orders in one leg. A read BEFORE the write was always right (the
    // accessor had nothing to observe yet); the bug was only visible AFTER,
    // which is exactly what makes the two disagree. The binding is one value,
    // so 3 + 3 — not 3 + 13.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        impl Feed {
            fun step(&mut self): i32 {
                if self is Feed::Ready(let items, let at) {
                    let before = at;
                    self = Feed::Ready(items, at + 10);
                    before + at
                } else {
                    -1
                }
            }
        }
        fun main() {
            mut feed = Feed::Ready(["a"], 3);
            print(feed.step());
        }
        "#,
        "6\n",
    );
}

#[test]
fn a_resource_capture_from_a_viewed_subject_loans_the_prematch_payload() {
    // R1/R11 and B65's doctrine, at the viewed subject. A resource payload has
    // no copy to make — "there is no user-facing copy spelling in vilan to
    // name" (affine-moves.md §9.1), and `x is Some(let r)` is always a LOAN
    // whatever the subject's form — so the capture is materialized WITHOUT
    // `__clone`: `const c = $a[1]`, which fixes WHICH value is loaned without
    // minting a second owner. That is what the place-subject twin below
    // already does, and matching it is the whole rule. 1 + 0, not 9 + 5.
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource struct Conn {
            id: i32,
        }
        enum Slot {
            Full(Conn, i32),
            Empty,
        }
        impl Slot {
            fun peek(&mut self): i32 {
                if self is Slot::Full(let c, let at) {
                    self = Slot::Full(Conn { id = 9 }, 5);
                    c.id + at
                } else {
                    -1
                }
            }
        }
        fun main() {
            mut slot = Slot::Full(Conn { id = 1 }, 0);
            print(slot.peek());
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_resource_capture_from_a_place_subject_loans_the_prematch_payload() {
    // The place twin the rule above is calibrated against: the same program
    // with an owned `mut` local reads the pre-assignment payload because the
    // assignment REBINDS and the subject temp keeps the old aggregate. It was
    // already right, and pinning it is what makes "indistinguishable from the
    // place path" a checked claim rather than a stated one.
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource struct Conn {
            id: i32,
        }
        enum Slot {
            Full(Conn, i32),
            Empty,
        }
        fun main() {
            mut slot = Slot::Full(Conn { id = 1 }, 0);
            if slot is Slot::Full(let c, let at) {
                slot = Slot::Full(Conn { id = 9 }, 5);
                print(c.id + at);
            }
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_readonly_view_subject_keeps_its_shared_accessors() {
    // The scope line, and the reason the predicate asks about WRITABILITY
    // rather than view-ness. Nothing can be written through a `&` view, so its
    // subject temp is a snapshot again and its captures keep the accessor —
    // and with it B53's SHARE elision, which exists to stop read-only walkers
    // deep-copying at every level. Widening the rule to every view would have
    // taken that back for `&self` methods.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        impl Feed {
            fun peek(&self): i32 {
                if self is Feed::Ready(let items, let at) {
                    items.len() + at
                } else {
                    -1
                }
            }
        }
        fun main() {
            let feed = Feed::Ready(["a", "b"], 5);
            print(feed.peek());
        }
        "#,
        "7\n",
    );
}

// ---------------------------------------------------------------------------
// B89 (the view write is a REPLACE, not a merge). Writing a whole aggregate
// through a view has to keep the pointee's identity — that is how the write
// reaches the caller — so it copies the value's slots into the pointee rather
// than rebinding. `Object.assign` did the copying, and `Object.assign` is a
// MERGE: a slot the value does not reach is left standing. Every aggregate
// whose width can shrink was wrong under it.
// ---------------------------------------------------------------------------

#[test]
fn a_shortening_write_through_a_list_view_truncates() {
    // The directly observable half, and the reason this is not merely an enum
    // bug: `Object.assign(v, [ 1 ])` over a three-element list overwrote slot 0
    // and left slots 1 and 2 alone, so the caller's list still had `len() == 3`
    // and still held `2` and `3`. Nothing in the source says "merge".
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun replace(v: &mut List<i32>) {
            v = [9];
        }
        fun main() {
            mut xs = [1, 2, 3];
            replace(&mut xs);
            print(xs.len());
            print(xs[0]);
        }
        "#,
        "1\n9\n",
    );
}

#[test]
fn a_shortening_reassign_of_a_viewed_enum_leaves_no_stale_payload() {
    // B89's filed repro. `self = Feed::Done` lowers to a write of `[ 1 ]` over
    // `[ 0, [ "a" ], 1 ]`; under the merge the payload survived in the trailing
    // slots — unreachable through the enum's own API (the tag gates every
    // read), but present, and for a RESOURCE payload it is a live object no
    // owner can reach. The emitted shape is the pin: the truncating write, and
    // the helper that truncates.
    let source = r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        impl Feed {
            fun finish(&mut self) {
                self = Feed::Done;
            }
        }
        fun main() {
            mut feed = Feed::Ready(["a"], 1);
            feed.finish();
            if feed is Feed::Done {
                print("done");
            }
        }
    "#;
    assert_emits_containing(source, "__replace(self, [ 1 ]);");
    assert_emits_containing(
        source,
        "if (Array.isArray(target) && Array.isArray(value)) target.length = value.length;",
    );
    assert_compiles_and_runs(source, "done\n");
}

#[test]
fn a_widening_reassign_of_a_viewed_enum_fills_every_slot() {
    // The other direction, which the fix must not break: the pointee GROWS, so
    // setting the length first opens holes that the copy then fills. A payload
    // read back after the widening write proves no slot was left a hole.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Feed {
            Ready(List<str>, i32),
            Done,
        }
        impl Feed {
            fun start(&mut self) {
                self = Feed::Ready(["a", "b"], 7);
            }
        }
        fun main() {
            mut feed = Feed::Done;
            feed.start();
            if feed is Feed::Ready(let items, let at) {
                print(items.len());
                print(at);
            }
        }
        "#,
        "2\n7\n",
    );
}

#[test]
fn an_equal_width_write_through_a_struct_view_is_unchanged() {
    // The width-fixed case, pinned so the general form stays a superset of the
    // merge it replaced: a struct's slots are the same on both sides, so the
    // merge happened to be right there and the replace must agree with it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32, y: i32 }
        fun move_to(p: &mut Point) {
            p = Point { x = 7, y = 8 };
        }
        fun main() {
            mut point = Point { x = 1, y = 2 };
            move_to(&mut point);
            print(point.x);
            print(point.y);
        }
        "#,
        "7\n8\n",
    );
}

#[test]
fn a_shortening_write_through_a_view_truncates_under_const_eval() {
    // The const-eval interpreter runs the SAME emitted nodes, so it needs the
    // replace natively — a merge there would fold a stale slot into a literal.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun replace(v: &mut List<i32>): i32 {
            v = [9];
            v.len()
        }
        fun main() {
            print(const {
                mut xs = [1, 2, 3];
                replace(&mut xs)
            });
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_view_write_drops_the_overwritten_variants_resource() {
    // B94, and the half B89's truncation did NOT fix. R2 (destruction.md §5)
    // says assigning onto a place that still holds a resource drops the old
    // value first, and the OWNED-place twin always implemented it:
    // `holder = Holder::Empty` on a local emits the tag-dispatching drop glue
    // before the write. Through a `&mut` view it did not — the scan that plans
    // overwrite drops tracks BINDINGS the scanned body owns, and a loan owns
    // nothing, so nothing was planned and the scope-end glue then read the NEW
    // tag and found nothing to drop. The guard was silently leaked (this
    // program printed "before\nafter"). RULED 2026-08-07: the loan drops what
    // it overwrites, twinning the owned path — the same indistinguishability
    // doctrine B81/B88 applied to captures.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop {
            fun drop(&mut self) {
                print(i"dropped {self.label}");
            }
        }
        enum Holder {
            Full(Guard),
            Empty,
        }
        impl Holder {
            fun clear(&mut self) {
                self = Holder::Empty;
            }
        }
        fun main() {
            mut holder = Holder::Full(Guard { label = "held" });
            print("before");
            holder.clear();
            print("after");
        }
        "#,
        "before\ndropped held\nafter\n",
    );
}

// ---------------------------------------------------------------------------
// B94 (R2 through a LOAN). The rule: a write through a writable view drops the
// pointee's outgoing value, exactly as the owned-place twin drops the
// binding's. Every pin below has its owned twin's answer as the expectation,
// because "indistinguishable from the owned path" IS the rule — the two write
// the same storage under different names. See proposal/destruction.md §5 (R2)
// and proposal/capture-clones.md §8.
// ---------------------------------------------------------------------------

#[test]
fn a_view_write_of_the_same_variant_width_drops_the_old_payload() {
    // Width was never the mechanism — B89's truncation only made the shrinking
    // shape LOOK like a width bug. `Full(g1)` -> `Full(g2)` overwrites the same
    // two slots and leaked just as hard.
    let program = |write: &str| {
        format!(
            r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard {{ label: str }}
        impl Guard with Drop {{ fun drop(&mut self) {{ print(i"dropped {{self.label}}"); }} }}
        enum Holder {{ Full(Guard), Empty }}
        impl Holder {{ fun swap(&mut self) {{ self = Holder::Full(Guard {{ label = "second" }}); }} }}
        fun main() {{
            mut holder = Holder::Full(Guard {{ label = "first" }});
            print("before");
            {write}
            print("after");
        }}
        "#
        )
    };
    // The owned twin first, so the expectation below is its answer verbatim.
    assert_compiles_and_runs(
        &program(r#"holder = Holder::Full(Guard { label = "second" });"#),
        "before\ndropped first\ndropped second\nafter\n",
    );
    assert_compiles_and_runs(
        &program("holder.swap();"),
        "before\ndropped first\ndropped second\nafter\n",
    );
}

#[test]
fn a_view_write_that_grows_the_variant_drops_the_old_payload() {
    // The other width direction: a one-slot payload replaced by a three-slot
    // one. `__replace` GROWS the array here, so nothing is truncated and the
    // old payload would survive in a reachable slot — the drop is owed for the
    // ownership reason, not the layout one.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Small(Guard), Big(Guard, i32, i32), Empty }
        impl Holder {
            fun grow(&mut self) { self = Holder::Big(Guard { label = "big" }, 1, 2); }
        }
        fun main() {
            mut holder = Holder::Small(Guard { label = "small" });
            print("before");
            holder.grow();
            print("after");
        }
        "#,
        "before\ndropped small\ndropped big\nafter\n",
    );
}

#[test]
fn a_view_write_to_a_struct_pointee_drops_the_old_value() {
    // No enum tag anywhere: the pointee is the resource itself, so the glue is
    // an unconditional call rather than a tag test. Same answer as
    // `overwrite_drops_the_old_value_then_the_new_at_scope_end`, one loan over.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        fun reset(g: &mut Guard) { g = Guard { label = "new" }; }
        fun main() {
            mut guard = Guard { label = "old" };
            print("before");
            reset(&mut guard);
            print("after");
        }
        "#,
        "before\ndropped old\ndropped new\nafter\n",
    );
}

#[test]
fn a_view_write_drops_before_the_truncating_replace_clobbers_the_payload() {
    // The B89 interaction, pinned in BYTES because the runtime answer alone
    // cannot see the order. `__replace` sets `target.length = value.length`
    // before merging, so a shrinking write deletes the payload slots outright —
    // a drop emitted after it would destroy nothing. The resource here sits in
    // slot 2, BEHIND a `List` payload, so the truncation to one slot takes both.
    let source = r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Full(List<i32>, Guard), Empty }
        impl Holder { fun clear(&mut self) { self = Holder::Empty; } }
        fun main() {
            mut holder = Holder::Full([1, 2, 3], Guard { label = "held" });
            print("before");
            holder.clear();
            print("after");
        }
        "#;
    match compile(source) {
        Ok(js) => {
            let drop_at = js.find("$a(self);").expect("the overwrite drop is emitted");
            let write_at = js
                .find("__replace(self,")
                .expect("the truncating write is emitted");
            assert!(
                drop_at < write_at,
                "the overwrite drop must precede the truncating write:\n{js}"
            );
        }
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "before\ndropped held\nafter\n");
}

#[test]
fn a_view_write_drops_the_payload_in_the_owned_paths_order() {
    // Order is part of the doctrine, not an accident of it: a variant with two
    // resource slots destroys them in reverse declaration order
    // (destruction.md §5), and the loan path must print the same sequence as
    // the owned one because it runs the same per-type glue.
    let program = |write: &str| {
        format!(
            r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard {{ label: str }}
        impl Guard with Drop {{ fun drop(&mut self) {{ print(i"dropped {{self.label}}"); }} }}
        enum Holder {{ Pair(Guard, Guard), Empty }}
        impl Holder {{ fun clear(&mut self) {{ self = Holder::Empty; }} }}
        fun main() {{
            mut holder = Holder::Pair(Guard {{ label = "a" }}, Guard {{ label = "b" }});
            print("before");
            {write}
            print("after");
        }}
        "#
        )
    };
    assert_compiles_and_runs(
        &program("holder = Holder::Empty;"),
        "before\ndropped b\ndropped a\nafter\n",
    );
    assert_compiles_and_runs(
        &program("holder.clear();"),
        "before\ndropped b\ndropped a\nafter\n",
    );
}

#[test]
fn a_view_write_through_a_mut_parameter_drops_the_old_value() {
    // The `&mut x` parameter spelling of the same write — `&mut self` is not a
    // special case, it is one of three names for a writable view.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Full(Guard), Empty }
        fun clear(v: &mut Holder) { v = Holder::Empty; }
        fun main() {
            mut holder = Holder::Full(Guard { label = "held" });
            print("before");
            clear(&mut holder);
            print("after");
        }
        "#,
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn a_view_write_through_a_mut_local_drops_the_old_value() {
    // The third spelling: a LOCAL bound to a `&mut`. `*v = ..` is not the
    // vilan surface ("a view is written through directly"), so the write is
    // spelled `v = ..` and `view_binding_mutability` is what recognizes it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Full(Guard), Empty }
        fun main() {
            mut holder = Holder::Full(Guard { label = "held" });
            let v = &mut holder;
            print("before");
            v = Holder::Empty;
            print("after");
        }
        "#,
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn a_view_write_through_a_nested_reborrow_drops_the_old_value() {
    // Two loans deep: `outer` re-borrows its own `&mut` into `inner`, which
    // does the write. The question is asked where the write is, so depth costs
    // nothing — but a rule keyed on "the receiver is `self`" would miss this.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Full(Guard), Empty }
        fun inner(v: &mut Holder) { v = Holder::Empty; }
        fun outer(v: &mut Holder) { inner(&mut v); }
        fun main() {
            mut holder = Holder::Full(Guard { label = "held" });
            print("before");
            outer(&mut holder);
            print("after");
        }
        "#,
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn repeated_view_writes_drop_each_outgoing_value_exactly_once() {
    // The double-drop question asked where it CAN be reached: two writes
    // through one loan. Each drops what it finds, and what the second finds is
    // what the first installed — the glue reads the pointee's CURRENT contents,
    // never a remembered value. "first" and "second" appear once each.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Full(Guard), Empty }
        impl Holder {
            fun churn(&mut self) {
                self = Holder::Full(Guard { label = "second" });
                self = Holder::Empty;
            }
        }
        fun main() {
            mut holder = Holder::Full(Guard { label = "first" });
            print("before");
            holder.churn();
            print("after");
        }
        "#,
        "before\ndropped first\ndropped second\nafter\n",
    );
}

#[test]
fn a_view_write_after_the_owner_moved_out_is_rejected() {
    // Why the loan arm asks no liveness question, pinned rather than argued.
    // The owned arm needs one because a body can move its own binding out and
    // must not then drop it twice; the loan arm cannot reach that state,
    // because minting the loan of a moved-out binding is itself a use-after-move
    // R1 already rejects. The owned twin of this program compiles and prints no
    // second drop (`a_moved_out_binding_is_not_overwrite_dropped`).
    assert_fails_with(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Full(Guard), Empty }
        impl Holder { fun clear(&mut self) { self = Holder::Empty; } }
        fun main() {
            mut holder = Holder::Full(Guard { label = "held" });
            match holder {
                Holder::Full(let g) => { print("took"); }
                Holder::Empty => {}
            }
            holder.clear();
        }
        "#,
        "after it was moved",
    );
}

#[test]
fn a_moved_out_binding_is_not_overwrite_dropped() {
    // The owned twin of the pin above, and the reason the owned arm keeps its
    // flow-sensitive `owned` set: the match consumed the payload (B62 destroys
    // it at the leg's end), so the assignment that follows overwrites a binding
    // that owns nothing and must NOT drop. "dropped held" appears once.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"dropped {self.label}"); } }
        enum Holder { Full(Guard), Empty }
        fun main() {
            mut holder = Holder::Full(Guard { label = "held" });
            match holder {
                Holder::Full(let g) => { print("took"); }
                Holder::Empty => {}
            }
            print("before");
            holder = Holder::Empty;
            print("after");
        }
        "#,
        "took\ndropped held\nbefore\nafter\n",
    );
}

#[test]
fn a_mut_view_binding_of_a_resource_does_not_drop_it_at_scope_end() {
    // The same confusion in the other direction, found while proving B94 and
    // fixed by the same sentence: a loan owns nothing, so it destroys nothing
    // at its scope end either. References are TRANSPARENT — `&mut Holder` has
    // type `Holder` — so `let v = &mut holder` minted a resource-TYPED local
    // that the drop planner enrolled as an owner, and the emitted program
    // destroyed the borrowed value twice ("dropped held" printed twice, on the
    // struct pointee as well as the enum one).
    //
    // S3 (lifetimes.md §6) moved WHEN, never how many: the owner's last use is
    // the loan itself and `v` is never read, so the extension rule extends
    // nothing and the owner drops before "hi". Exactly one teardown, which is
    // the whole of what this pin holds.
    let program = |declaration: &str, borrow: &str| {
        format!(
            r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard {{ label: str }}
        impl Guard with Drop {{ fun drop(&mut self) {{ print(i"dropped {{self.label}}"); }} }}
        enum Holder {{ Full(Guard), Empty }}
        fun main() {{
            {declaration}
            let v = {borrow};
            print("hi");
        }}
        "#
        )
    };
    assert_compiles_and_runs(
        &program(
            r#"mut holder = Holder::Full(Guard { label = "held" });"#,
            "&mut holder",
        ),
        "dropped held\nhi\n",
    );
    assert_compiles_and_runs(
        &program(r#"mut guard = Guard { label = "held" };"#, "&mut guard"),
        "dropped held\nhi\n",
    );
    // The read-only loan too — `&` was never writable, but it was just as
    // wrongly enrolled as an owner.
    assert_compiles_and_runs(
        &program(r#"mut guard = Guard { label = "held" };"#, "&guard"),
        "dropped held\nhi\n",
    );
}

#[test]
fn a_view_write_to_a_data_pointee_emits_no_drop() {
    // The negative half, in bytes: the rule fires on the pointee's RESOURCE-ness,
    // not on the fact that a write goes through a view. A data pointee's write
    // is the bare `__replace` it always was, which is what keeps every
    // resource-free corpus program byte-identical.
    assert_emits_containing(
        r#"
        import std::print;
        struct Holder { n: i32 }
        impl Holder { fun clear(&mut self) { self = Holder { n = 0 }; } }
        fun main() {
            mut holder = Holder { n = 7 };
            holder.clear();
            print(holder.n);
        }
        "#,
        "function clear(self) {\n\t__replace(self, [ 0 ]);\n}",
    );
}

// ---------------------------------------------------------------------------
// B99 (R2's COMPONENT half). B94 closed the loan half and filed this one: R2 is
// spelled over a BINDING and R5 over reading and moving a field, so writing
// over one fell between them and the outgoing value was leaked outright. The
// rule now reads over the PLACE all the way down — a write over a component
// whose type is a resource drops what it replaces, owned place and view alike,
// with no liveness question (R5 makes a resource field loan-only, so a
// component place always holds a live value). See proposal/destruction.md R2
// and capture-clones.md §10.
// ---------------------------------------------------------------------------

/// The B99 pin family's shared preamble: a `Drop`-printing resource, an enum
/// that holds one, and a struct that holds the enum.
fn b99_program(body: &str) -> String {
    format!(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard {{ label: str }}
        impl Guard with Drop {{ fun drop(&mut self) {{ print(i"dropped {{self.label}}"); }} }}
        enum Holder {{ Full(Guard), Empty }}
        struct Slot {{ held: Holder }}
        {body}
        "#
    )
}

/// [`b99_program`] compiled, for the pins whose claim is about emitted BYTES.
fn b99_js(body: &str) -> String {
    match compile(&b99_program(body)) {
        Ok(js) => js,
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
}

#[test]
fn a_component_write_to_a_resource_field_drops_the_old_value() {
    // The filed repro (B94's bycatch), now closed. R2 was implemented over the
    // BINDING — only a whole-binding target enrolled — so `slot.held = ..`
    // overwrote a live resource that no rule covered and printed
    // "before\nafter", leaking the guard. Whether the write is legal at all is
    // R5's question and R5 permits it (it makes a field loan-only and rejects
    // moving one OUT; it says nothing about writing over one), which is exactly
    // why R2 owes the drop.
    assert_compiles_and_runs(
        &b99_program(
            r#"
        fun main() {
            mut slot = Slot { held = Holder::Full(Guard { label = "held" }) };
            print("before");
            slot.held = Holder::Empty;
            print("after");
        }
        "#,
        ),
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn a_nested_component_write_drops_the_old_value() {
    // The projection is asked of the PLACE, not of its depth: `o.inner.held`
    // names the storage the write clobbers exactly as `slot.held` does.
    assert_compiles_and_runs(
        &b99_program(
            r#"
        struct Outer { inner: Slot }
        fun main() {
            mut o = Outer { inner = Slot { held = Holder::Full(Guard { label = "held" }) } };
            print("before");
            o.inner.held = Holder::Empty;
            print("after");
        }
        "#,
        ),
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn a_component_write_through_a_view_drops_the_old_value() {
    // B94's arm composes, and this is the doctrine's own test: the two
    // spellings are the same expression shape and differ only in what the root
    // binding is, so the answer must not depend on the root. The predicate is
    // the component's type, and it never asks.
    assert_compiles_and_runs(
        &b99_program(
            r#"
        fun clear(s: &mut Slot) { s.held = Holder::Empty; }
        fun main() {
            mut slot = Slot { held = Holder::Full(Guard { label = "held" }) };
            print("before");
            clear(&mut slot);
            print("after");
        }
        "#,
        ),
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn a_tuple_component_write_drops_the_old_value() {
    // The same rule at the other positional spelling.
    assert_compiles_and_runs(
        &b99_program(
            r#"
        fun main() {
            mut pair = (1, Holder::Full(Guard { label = "held" }));
            print("before");
            pair.1 = Holder::Empty;
            print("after");
        }
        "#,
        ),
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn an_element_write_drops_the_old_value() {
    // The `Index` spelling — a fixed array of resources, which is the only
    // indexable resource aggregate (R10 rejects the native containers). The
    // trailing "dropped two"/"dropped three" are the scope-end teardown, in
    // reverse element order.
    assert_compiles_and_runs(
        &b99_program(
            r#"
        fun main() {
            mut arr: [Guard; 2] = [Guard { label = "one" }, Guard { label = "two" }];
            print("before");
            arr[0] = Guard { label = "three" };
            print("after");
        }
        "#,
        ),
        "before\ndropped one\ndropped two\ndropped three\nafter\n",
    );
}

#[test]
fn a_component_write_inside_a_match_arm_drops_the_old_value() {
    // The write is planned by `plan_expr`, which descends every arm, so a
    // conditional write is covered without a liveness question — the component
    // place is live on every path that reaches it.
    assert_compiles_and_runs(
        &b99_program(
            r#"
        fun main() {
            mut slot = Slot { held = Holder::Full(Guard { label = "held" }) };
            let n = 1;
            print("before");
            match n { 1 => { slot.held = Holder::Empty; } _ => {} }
            print("after");
        }
        "#,
        ),
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn a_component_write_to_a_data_field_emits_no_drop() {
    // The negative half, in bytes: the rule fires on the COMPONENT's
    // resource-ness, not on the fact that its aggregate holds a resource
    // somewhere. `count` is an `i32` in a struct that also holds a `Holder`, and
    // its write is the bare slot assignment it always was — which is what keeps
    // every resource-free corpus program byte-identical.
    assert_emits_containing(
        &b99_program(
            r#"
        struct Pair { held: Holder, count: i32 }
        fun main() {
            mut pair = Pair { held = Holder::Full(Guard { label = "held" }), count = 1 };
            pair.count = 5;
            print(pair.count);
        }
        "#,
        ),
        "pair[1] = 5;",
    );
}

#[test]
fn a_component_write_drops_before_the_write_clobbers_the_slot() {
    // The ordering is load-bearing rather than cosmetic, for the same reason
    // B94 pinned it on the view path: the drop's operand IS the slot the write
    // replaces, so a drop emitted afterwards would destroy the NEW value.
    // Pinned in bytes, since a runtime pin cannot tell "dropped the old one"
    // from "dropped a same-shaped new one".
    let js = b99_js(
        r#"
        fun main() {
            mut slot = Slot { held = Holder::Full(Guard { label = "one" }) };
            slot.held = Holder::Full(Guard { label = "two" });
        }
        "#,
    );
    let drop_index = js.find("(slot[0]);").expect("the overwrite drop");
    let write_index = js.find("slot[0] = [ 0,").expect("the component write");
    assert!(
        drop_index < write_index,
        "the drop must precede the write that clobbers the slot it reads:\n{js}"
    );
}

#[test]
fn a_component_write_drops_in_the_owned_twins_order() {
    // The whole-binding twin of the same program, so the two spellings can be
    // compared: both destroy the outgoing value BEFORE the incoming one is
    // installed, and neither destroys the incoming one until its scope ends.
    let component = &b99_program(
        r#"
        fun main() {
            mut slot = Slot { held = Holder::Full(Guard { label = "one" }) };
            print("before");
            slot.held = Holder::Full(Guard { label = "two" });
            print("after");
        }
        "#,
    );
    let binding = &b99_program(
        r#"
        fun main() {
            mut held = Holder::Full(Guard { label = "one" });
            print("before");
            held = Holder::Full(Guard { label = "two" });
            print("after");
        }
        "#,
    );
    assert_compiles_and_runs(component, "before\ndropped one\ndropped two\nafter\n");
    assert_compiles_and_runs(binding, "before\ndropped one\ndropped two\nafter\n");
}

#[test]
fn a_write_through_a_view_of_a_component_still_drops() {
    // The neighbour that was ALREADY right, pinned so the new arm cannot move
    // it: `&mut slot.held` mints a view whose own write is B94's loan arm, and
    // it takes the `__replace` path (a whole-value write through a view) rather
    // than this one. The two spellings agree, which is the point.
    assert_compiles_and_runs(
        &b99_program(
            r#"
        fun main() {
            mut slot = Slot { held = Holder::Full(Guard { label = "held" }) };
            print("before");
            let v = &mut slot.held;
            v = Holder::Empty;
            print("after");
        }
        "#,
        ),
        "before\ndropped held\nafter\n",
    );
}

#[test]
fn a_component_write_drops_before_the_truncating_replace_on_the_view_path() {
    // B89's interaction, unchanged by B99 and pinned here because the component
    // arm shares the emission site: a write THROUGH a view still lowers to
    // `__replace`, which sets `target.length = value.length` before merging, so
    // a drop emitted after it would destroy slots that no longer exist. A
    // component write is a plain slot assignment and never truncates — the
    // ordering is owed for the simpler reason above — so both orders are pinned
    // from the one emission.
    let js = b99_js(
        r#"
        fun refill(held: &mut Holder) { held = Holder::Empty; }
        fun main() {
            mut slot = Slot { held = Holder::Full(Guard { label = "held" }) };
            refill(&mut slot.held);
        }
        "#,
    );
    let drop_index = js.find("(held);").expect("the overwrite drop");
    let replace_index = js.find("__replace(held,").expect("the truncating write");
    assert!(
        drop_index < replace_index,
        "the drop must precede `__replace`'s truncation (B89):\n{js}"
    );
}

#[test]
fn a_resource_free_component_write_plans_no_drop_pass_at_all() {
    // The early return that keeps resource-free programs byte-identical: a
    // program with no declared resource has no resource type, so the component
    // arm collects nothing and no `try`/`finally` appears.
    let js = match compile(
        r#"
        import std::print;
        struct Slot { held: i32 }
        fun main() {
            mut slot = Slot { held = 1 };
            slot.held = 2;
            print(slot.held);
        }
        "#,
    ) {
        Ok(js) => js,
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    };
    assert!(
        !js.contains("finally"),
        "a resource-free program takes no teardown:\n{js}"
    );
}

// ---------------------------------------------------------------------------
// B88 (the same seam through an OWNED place). B81 closed the alias path's late
// read for a writable-view subject and stopped there, because an owned place
// looked safe: assigning it REBINDS it, installing a fresh value and leaving
// the subject temp holding the old one. That is true of exactly one write
// form. A COMPONENT write — `t.1 = 9`, `h.pair.1 = 9`, `xs[0] = 9` — mutates
// the very object the temp aliases, and so does a `&mut` taken of the place
// and a `&mut self` method called on it. Captures of a subject some in-place
// write can reach are now read once, at the match, exactly as a viewed
// subject's are. See proposal/capture-clones.md §7.
// ---------------------------------------------------------------------------

#[test]
fn an_is_capture_from_a_component_written_place_reads_the_prematch_value() {
    // B88's filed repro. `b` is an `i32`, so it owes no copy and kept its
    // accessor `$a[1]`; `t.1 = 99` lowers to `t[1] = 99`, and `$a` IS `t`'s
    // array, so the deferred read returned the write. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut t = (7, 3);
            if t is (let a, let b) {
                t.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_place_capture_read_before_and_after_a_component_write_agrees() {
    // Both orders in one leg, the shape that makes the bug undeniable: a read
    // BEFORE the write was always right (the accessor had nothing to observe
    // yet), so the two reads of one binding DISAGREED — 3 then 99. The
    // binding is one value, so 6, not 102.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut t = (7, 3);
            if t is (let a, let b) {
                let before = b;
                t.1 = 99;
                print(before + b);
            }
        }
        "#,
        "6\n",
    );
}

#[test]
fn a_component_write_through_a_field_path_does_not_reach_a_capture() {
    // The field arm. A struct field is not patternable, so a field write
    // reaches this seam through the subject's PATH rather than the capture's:
    // the subject is `h.pair`, a `Field` place, and `h.pair.1 = 99` writes
    // into the very array that place names. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32), tag: i32 }
        fun main() {
            mut h = Holder { pair = (7, 3), tag = 0 };
            if h.pair is (let a, let b) {
                h.pair.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_disjoint_field_write_leaves_a_sibling_subject_correct() {
    // The other side of the field arm, and the honest record of how coarse
    // the predicate is: the write lands in a DIFFERENT field of the same root,
    // so it could never have reached this subject and the program was already
    // right. Root granularity materializes it anyway — a write that reaches
    // the storage under a second name got that name from a `&mut` of the
    // ROOT, so the root is the granularity the question can be asked at
    // soundly. Pinned because the answer must not change either way.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32), tag: i32 }
        fun main() {
            mut h = Holder { pair = (7, 3), tag = 5 };
            if h.pair is (let a, let b) {
                h.tag = 1;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn an_index_write_does_not_reach_a_fixed_array_capture() {
    // The index arm, at the fixed-array binder — `marr[1] = 99` lowers to
    // `__at_put(marr, 1, 99)`, an in-place element store into the array the
    // subject temp aliases. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut marr: [i32; 2] = [7, 3];
            if marr is let [g, k] {
                marr[1] = 99;
                print(k);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn an_index_write_does_not_reach_a_capture_of_an_indexed_subject() {
    // The index arm at the SUBJECT instead: `rows[0]` is an `Index` place, and
    // `rows[0].1 = 99` writes the element it names. `place_root` walks the
    // subscript, so both sides root at `rows`. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut rows = [(7, 3)];
            if rows[0] is (let a, let b) {
                rows[0].1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_nested_component_write_does_not_reach_a_nested_capture() {
    // Depth on both sides at once: the capture sits inside a nested tuple
    // pattern and the write reaches it through two components (`n.0.1`).
    // Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut n = ((7, 3), 5);
            if n is ((let i, let j), let k) {
                n.0.1 = 99;
                print(j);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_mut_self_method_call_does_not_reach_a_capture_of_its_receiver() {
    // The write need not be spelled in the arm at all. `bump` takes `&mut
    // self` and writes a component through it, so the CALL is the in-place
    // write and the receiver's root joins the write set — the same arm of
    // `collect_written_roots` the `own`-parameter capture pins above use.
    // Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Counter { pair: (i32, i32) }
        impl Counter {
            fun bump(&mut self) { self.pair.1 = 99 }
        }
        fun main() {
            mut counter = Counter { pair = (7, 3) };
            if counter.pair is (let p, let q) {
                counter.bump();
                print(q);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_write_through_a_mut_view_of_the_subject_does_not_reach_its_captures() {
    // Why the question is asked at the ROOT and not by walking the arm: the
    // view is minted OUTSIDE the arm and written INSIDE it, so the write's own
    // place root is `vv`, not `vt`, and an arm-local write-set walk would see
    // nothing to report. Taking the `&mut` is itself a recorded write of
    // `vt`, which is what makes the root question sound. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut vt = (7, 3);
            let vv = &mut vt;
            if vt is (let a, let b) {
                vv.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_guarded_leg_capture_from_a_component_written_place_reads_the_prematch_value() {
    // A guard puts the leg on the alias path too, and its captures carry the
    // same hole. It is also the ordering-sensitive one (B59): the guard is
    // walked after `materialize_captures` has re-pointed the alias table, so a
    // guard that reads a materialized capture takes the prelude shape or names
    // a binding that has not been declared. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut gl = (7, 3);
            match gl {
                (let q, let r) if r > 0 => {
                    gl.1 = 99;
                    print(r);
                }
                _ => {}
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn an_unguarded_match_leg_on_a_component_written_place_was_already_right() {
    // The negative half of the diagnosis, the same one B81 pinned through a
    // view: an unguarded leg compiles through `compile_pattern`, which
    // declares every capture as a real `const` at leg entry, so it never read
    // late and never had the bug. Pinned so the fix cannot move it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut ml = (7, 3);
            match ml {
                (let s, let v) => {
                    ml.1 = 99;
                    print(v);
                }
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_destructure_from_a_component_written_place_was_already_right() {
    // The declared path's other spelling, for the same reason: `let (a, b) =
    // t` reads both slots eagerly, so a later component write has nothing to
    // reach back into.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut d = (7, 3);
            let (d1, d2) = d;
            d.1 = 99;
            print(d2);
        }
        "#,
        "3\n",
    );
}

#[test]
fn both_capture_shapes_survive_a_component_write_to_the_place() {
    // The place twin of `both_capture_shapes_survive_an_in_place_write_
    // through_the_view`, which is what makes "the two paths are
    // indistinguishable per shape" a checked claim: the same two payload
    // shapes, the same two component writes, an owned `mut` local instead of a
    // `&mut` parameter, and the same answers. `items` is B53's business (an
    // aggregate capture COPIES, so growing the source to 4 leaves it at 3);
    // `at` is B88's (a scalar owes no copy, so it kept an accessor and read
    // the 9). One pin, red either way: 12 without the materialization, 4
    // without the copy.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut pair = (["a", "b", "c"], 0);
            if pair is (let items, let at) {
                pair.0.push("d");
                pair.1 = 9;
                print(items.len() + at);
            }
            print(pair.0.len());
        }
        "#,
        "3\n4\n",
    );
}

#[test]
fn a_resource_capture_from_a_component_written_place_loans_the_prematch_payload() {
    // R1/R11 and B65's doctrine at the third subject form. A resource payload
    // has no copy to make — "there is no user-facing copy spelling in vilan to
    // name" (affine-moves.md §9.1) — so the capture is materialized WITHOUT
    // `__clone`: `const c = $a[0]`, which fixes WHICH value is loaned without
    // minting a second owner to destroy twice. Exactly what the viewed twin
    // does, and what the whole-assignment place twin already did. 1, not 6.
    let source = r#"
        import std::print;
        resource struct Conn { id: i32 }
        fun main() {
            mut slot = (Conn { id = 1 }, 0);
            if slot is (let c, let at) {
                slot.1 = 5;
                print(c.id + at);
            }
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "the resource capture was materialized WITH a copy:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "1\n");
}

#[test]
fn a_whole_assignment_to_the_subject_still_leaves_its_captures_aliasing() {
    // The line the rule stops at, and the reason it is not simply "every place
    // subject materializes". Assigning the whole binding REBINDS it — `t = [
    // 1, 2 ]` installs a fresh array and the subject temp keeps the old one —
    // so the accessor is a faithful snapshot and there is nothing to fix. The
    // capture must therefore stay an accessor: naming it in the output would
    // mean the predicate had widened to every place subject, which is what
    // moves six corpus goldens and takes back B53's share elision on the alias
    // path.
    let source = r#"
        import std::print;
        fun main() {
            mut t = (7, 3);
            if t is (let first, let kept) {
                t = (1, 2);
                print(kept);
            }
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("kept"),
            "a rebinding assignment materialized the capture anyway:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "3\n");
}

// ---------------------------------------------------------------------------
// B97 (the THIRD subject spelling: a `borrows` CALL). A method returning a
// `&mut` projection hands the pattern a subject that aliases the receiver's
// storage, but the expression is a CALL — and `is_capture_subject_place`
// admitted places and `*view` only, so the subject collected no candidates at
// all and BOTH rules were missing at once: B53's copy and B81/B88's
// materialization. Measured before shipping (proposal/capture-clones.md §9),
// and the doctrine per payload shape is twinned below onto this path.
// ---------------------------------------------------------------------------

#[test]
fn a_borrows_call_subject_reads_the_prematch_value() {
    // The VALUE half of the doctrine (§7.5): a scalar owes no copy, so the
    // declaration alone fixes the timing. Before B97 this subject collected no
    // candidates whatever — `const $a = slot(h); … $a[1]` — so the capture
    // re-read the mutated slot and printed 99, the same shape §6.4 found for
    // `*view`, one spelling over.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun slot(&mut self): &mut (i32, i32) borrows self { &mut self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            if h.slot() is (let a, let b) {
                h.pair.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_borrows_call_subject_copies_its_captures() {
    // The AGGREGATE half (§7.5), and the worse of the two because it is B53's
    // ORIGINAL bug rather than a timing one: the capture aliased the
    // receiver's element outright (no `__clone` anywhere in the output), so
    // growing the source through the receiver grew the capture. Printed 3.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { cells: (List<i32>, i32) }
        impl Holder {
            fun slot(&mut self): &mut (List<i32>, i32) borrows self { &mut self.cells }
        }
        fun main() {
            mut g = Holder { cells = ([1, 2], 3) };
            if g.slot() is (let xs, let n) {
                g.cells.0.push(9);
                print(xs.len());
            }
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_resource_capture_from_a_borrows_call_subject_loans_the_prematch_payload() {
    // The RESOURCE half of the doctrine (§7.5), twinned onto the third path.
    // R1 forbids the copy and B65 forbids inventing one ("there is no
    // user-facing copy spelling in vilan to name", affine-moves.md §9.1), so
    // the capture is materialized BARE — `const c = $a[0]`, no `__clone` — which
    // fixes WHICH value is loaned without minting a second owner to destroy
    // twice. Both halves asserted: the value (1, not 6) and the absent copy.
    let source = r#"
        import std::print;
        resource struct Conn { id: i32 }
        struct Holder { slot: (Conn, i32) }
        impl Holder {
            fun view(&mut self): &mut (Conn, i32) borrows self { &mut self.slot }
        }
        fun main() {
            mut holder = Holder { slot = (Conn { id = 1 }, 0) };
            if holder.view() is (let c, let at) {
                holder.slot.1 = 5;
                print(c.id + at);
            }
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "the resource capture was materialized WITH a copy:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "1\n");
}

#[test]
fn both_capture_shapes_survive_a_write_through_a_borrows_call_subject() {
    // The two-shape twin of `both_capture_shapes_survive_a_component_write_to_
    // the_place` and its viewed sibling — what makes "the three paths are
    // indistinguishable per shape" a checked claim rather than a story. Same
    // two payload shapes, same two writes, a `borrows`-call subject instead of
    // a place or a `&mut` parameter, same answers. Red on either axis: 12
    // without the materialization, 4 without the copy.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (List<str>, i32) }
        impl Holder {
            fun view(&mut self): &mut (List<str>, i32) borrows self { &mut self.pair }
        }
        fun main() {
            mut holder = Holder { pair = (["a", "b", "c"], 0) };
            if holder.view() is (let items, let at) {
                holder.pair.0.push("d");
                holder.pair.1 = 9;
                print(items.len() + at);
            }
            print(holder.pair.0.len());
        }
        "#,
        "3\n4\n",
    );
}

#[test]
fn a_readonly_borrows_call_subject_materializes_when_a_write_reaches_the_receiver() {
    // Why the rule is not simply `returns_mut_view`, measured (§9): a `&`
    // projection cannot be written THROUGH, but the receiver it projects can
    // still be written under its own name while the leg is live, and the temp
    // aliases the receiver's storage either way. B81's writable-view arm does
    // not cover this, so the second arm asks B88's root question — of the
    // arguments the callee projects, which is where the receiver is visible.
    // Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun peek(&self): &(i32, i32) borrows self { &self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            if h.peek() is (let a, let b) {
                h.pair.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_free_function_borrows_call_subject_reads_the_prematch_value() {
    // The receiver is not special — `borrows` names a parameter POSITION, and
    // the projection is read at the call site from whatever argument sits
    // there. A rule keyed on "the subject is a method call on a place" would
    // miss this. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun slot(h: &mut Holder): &mut (i32, i32) borrows h { &mut h.pair }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            if slot(&mut h) is (let a, let b) {
                h.pair.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_guarded_leg_over_a_borrows_call_subject_reads_the_prematch_value() {
    // B59's placement question on the third path: a guard that reads a
    // materialized capture takes the prelude shape, and the leg still sees the
    // pre-write value. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun slot(&mut self): &mut (i32, i32) borrows self { &mut self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            print(match h.slot() {
                (let a, let b) if b > 0 => {
                    h.pair.1 = 99;
                    b
                }
                _ => 0,
            });
        }
        "#,
        "3\n",
    );
}

#[test]
fn an_unguarded_match_over_a_borrows_call_subject_copies_its_aggregate_capture() {
    // The unguarded leg never had the TIMING bug — `compile_pattern` declares
    // every capture as a real `const` at leg entry — but it had the COPY one,
    // because the copy question is settled by the candidate set both paths
    // share. Printed 3.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { cells: (List<i32>, i32) }
        impl Holder {
            fun slot(&mut self): &mut (List<i32>, i32) borrows self { &mut self.cells }
        }
        fun main() {
            mut g = Holder { cells = ([1, 2], 3) };
            match g.slot() {
                (let xs, let n) => {
                    g.cells.0.push(9);
                    print(xs.len());
                }
            }
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_let_destructure_of_a_borrows_call_copies_its_aggregate_capture() {
    // §6.4's other half at this spelling: the capture pass gates
    // `Expr::Destructure` on the same predicate, so `let (xs, n) = h.slot()`
    // never copied either and growing the element through the receiver grew
    // the capture. Printed 3.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { cells: (List<i32>, i32) }
        impl Holder {
            fun slot(&mut self): &mut (List<i32>, i32) borrows self { &mut self.cells }
        }
        fun main() {
            mut g = Holder { cells = ([1, 2], 3) };
            let (xs, n) = g.slot();
            g.cells.0.push(9);
            print(xs.len());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_chained_borrows_projection_subject_reads_the_prematch_value() {
    // Why the writable-view arm is kept alongside the root question, measured
    // rather than argued (§9). A CHAINED projection's receiver is another
    // call, and a call has no place root — so the root arm finds nothing to
    // ask about and the subject would keep its accessor. B81's arm needs no
    // write to be found: a `&mut` projection is a writable view by
    // construction, whatever it was projected from. Printed 99 without it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { pair: (i32, i32) }
        struct Outer { inner: Inner }
        impl Outer {
            fun inner_mut(&mut self): &mut Inner borrows self { &mut self.inner }
        }
        impl Inner {
            fun slot(&mut self): &mut (i32, i32) borrows self { &mut self.pair }
        }
        fun main() {
            mut o = Outer { inner = Inner { pair = (7, 3) } };
            if o.inner_mut().slot() is (let a, let b) {
                o.inner.pair.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_readonly_projection_of_a_writable_one_reads_the_prematch_value() {
    // The same arm read ONE LEVEL UP, and the shape that shows the two halves
    // are not the same question. `peek()` returns `&`, so the outer call is
    // not itself a writable view; its receiver is `inner_mut()`, which is —
    // and the storage the temp aliases is therefore writable after all. The
    // root arm cannot reach it (the receiver is a call, so it has no root),
    // and the outer call's own convention says the wrong thing. Printed 99
    // without the recursion into the projected receiver.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { pair: (i32, i32) }
        struct Outer { inner: Inner }
        impl Outer {
            fun inner_mut(&mut self): &mut Inner borrows self { &mut self.inner }
        }
        impl Inner {
            fun peek(&self): &(i32, i32) borrows self { &self.pair }
        }
        fun main() {
            mut o = Outer { inner = Inner { pair = (7, 3) } };
            if o.inner_mut().peek() is (let a, let b) {
                o.inner.pair.1 = 99;
                print(b);
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_wrapped_view_capture_over_a_borrows_call_is_not_copied() {
    // The line the widening had to stop at, and the one the measurement found
    // (§9): admitting `borrows` calls newly reaches `Option<&mut T>` returns,
    // whose `Some(let v)` capture IS a view. References are transparent, so
    // `&mut List<i32>` is an aggregate by every type test in the pass and the
    // first candidate copied it — which deep-copies the borrowed storage, so
    // `v[0] = 77` landed on the copy and the caller's list never changed
    // (`option-view.vl`, 77 -> 1, a semantic break rather than a cost).
    // A view aliases on purpose: it never copies.
    let source = r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun inner_mut(&mut self): Option<&mut Inner> { Some(&mut self.inner) }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 1 } };
            match h.inner_mut() {
                Some(let v) => { v.n = 77; }
                None => {}
            }
            print(h.inner.n);
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "a view capture was copied, which breaks the alias:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "77\n");
}

#[test]
fn an_owned_call_subject_still_binds_without_copying() {
    // The elision the widening must not take back, in bytes: a call returning
    // an OWNED value produces storage of its own, so its elements have no
    // second owner and B53 §2's "destructuring a FRESH value binds without
    // copying" stands. `call_returns_view` is what separates the two, not
    // "the subject is a call".
    let source = r#"
        import std::print;
        fun make(): (List<i32>, i32) { ([1, 2], 3) }
        fun main() {
            if make() is (let xs, let n) {
                print(xs.len() + n);
            }
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "an owned call result's capture was copied:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "5\n");
}

#[test]
fn a_borrows_call_subject_with_no_write_in_the_leg_is_unchanged() {
    // The neighbour that was already right and must stay right: nothing writes
    // the receiver while the leg is live, so the aggregate capture's copy is
    // the only thing owed and the answer never depended on the timing.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { cells: (List<i32>, i32) }
        impl Holder {
            fun slot(&mut self): &mut (List<i32>, i32) borrows self { &mut self.cells }
        }
        fun main() {
            mut g = Holder { cells = ([1, 2], 3) };
            if g.slot() is (let xs, let n) {
                print(xs.len() + n);
            }
        }
        "#,
        "5\n",
    );
}

// ---------------------------------------------------------------------------
// B54 / A20 (rule 1 at the STORE seams). A place read into a slot of an
// aggregate that outlives the expression must copy: a construction literal's
// element/field/payload (B54), and the argument of a container method that
// keeps it (A20, via `own`). See proposal/element-clones.md.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// B100 (the return rule's loan hole). §3 of element-clones.md exempted a
// `&`/`&mut` parameter from the return copy, framed as "returning through one
// is rule 3's `borrows` projection, deliberately an alias". That is a fact
// about the RETURN, not about the place: a function whose signature hands back
// a VALUE hands back a value, whatever convention the place it read is under.
// R3's own list calls bare, `&` and `&mut` all loans, and the bare half already
// copied — the asymmetry was the bug. See proposal/element-clones.md §9.
// ---------------------------------------------------------------------------

#[test]
fn a_returned_field_place_copies_out_of_its_receiver() {
    // The filed repro (B97's bycatch), now closed. `fun make(&self): (i32, i32)
    // { self.pair }` emitted `return self[0]`, so the caller's result WAS the
    // receiver's field storage and a later write to the receiver showed through
    // it. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun make(&self): (i32, i32) { self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = h.make();
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_returned_field_place_copies_out_of_a_mut_receiver() {
    // `&mut self` is the same loan with a different writability, and the return
    // seam never asked about writability — the caller's storage outlives the
    // call either way.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun make(&mut self): (i32, i32) { self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = h.make();
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_returned_field_of_a_view_parameter_copies() {
    // The free-function spelling: nothing about this is special to a receiver.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun make(h: &Holder): (i32, i32) { h.pair }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = make(&h);
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_view_receiver_forwarded_whole_into_a_by_value_return_copies() {
    // The shape that decides how the exemption is spelled. Asking whether the
    // returned PLACE is a view answers the wrong question here — references are
    // transparent, so `self` inside `&self` is a view by every test — while the
    // signature says plainly that what leaves is a `Holder`, by value. The
    // exemption reads the signature.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun copy(&self): Holder { self }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let c = h.copy();
            h.pair.1 = 99;
            print(c.pair.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_returned_nested_field_of_a_receiver_copies() {
    // The copy lands at the LEAF, so the depth of the projection is irrelevant.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { pair: (i32, i32) }
        struct Holder { inner: Inner }
        impl Holder {
            fun make(&self): (i32, i32) { self.inner.pair }
        }
        fun main() {
            mut h = Holder { inner = Inner { pair = (7, 3) } };
            let p = h.make();
            h.inner.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_returned_list_field_of_a_receiver_copies() {
    // The A20 shape one seam over: the result's ELEMENTS were the receiver's,
    // so growing the result grew the receiver.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { items: List<i32> }
        impl Holder {
            fun items_of(&self): List<i32> { self.items }
        }
        fun main() {
            mut h = Holder { items = [1, 2] };
            mut xs = h.items_of();
            xs.push(9);
            print(h.items.len());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_returned_field_copies_in_the_tail_arm_that_owes_it() {
    // Keyed by the tail LEAF, so an `if`/`match` tail copies only in the arms
    // that hand back the receiver's storage — the constructed arm stays free.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun pick(&self, first: bool): (i32, i32) {
                if first { self.pair } else { (0, 0) }
            }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = h.pick(true);
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn an_early_ret_of_a_receivers_field_copies() {
    // The `ret` statement is a return seam like the tail is; both feed
    // `return_sites`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun make(&self, early: bool): (i32, i32) {
                if early { ret self.pair }
                (0, 0)
            }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = h.make(true);
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_borrows_projection_still_returns_the_alias() {
    // The elision the rule must not eat, and the reason the exemption exists at
    // all: a signature that returns `&mut T` hands back an alias on purpose
    // (rule 3's sanctioned escape), so writing through the result reaches the
    // receiver.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun slot(&mut self): &mut (i32, i32) borrows self { &mut self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            h.slot() = (1, 2);
            print(h.pair.0);
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_readonly_borrows_projection_still_returns_the_alias() {
    // The `&` half of the same exemption — a read-only projection is still a
    // projection, and its result must keep naming the receiver's storage.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun peek(&self): &(i32, i32) borrows self { &self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let v = h.peek();
            h.pair.1 = 99;
            print((*v).1);
        }
        "#,
        "99\n",
    );
}

#[test]
fn a_view_parameter_forwarded_into_a_view_return_still_aliases() {
    // The exemption's own shape, and the twin of the by-value case above: the
    // SAME body — a view parameter handed straight back — is an alias here and
    // a copy there, and only the signature separates them. This is what the
    // `returns_view` question is for; nothing else in the pass distinguishes
    // the two, because a `&mut place` leaf is not a place at all and never
    // reaches the seam.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun same(v: &mut Holder): &mut Holder borrows v { v }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let w = same(&mut h);
            w.pair.1 = 99;
            print(h.pair.1);
        }
        "#,
        "99\n",
    );
}

#[test]
fn a_receiver_forwarded_into_a_view_return_still_aliases() {
    // The method spelling of the same exemption, which is `copy(&self):
    // Holder { self }` with one word changed in the return type.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun me(&mut self): &mut Holder borrows self { self }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let w = h.me();
            w.pair.1 = 99;
            print(h.pair.1);
        }
        "#,
        "99\n",
    );
}

#[test]
fn a_bare_self_receivers_field_return_was_already_right() {
    // The neighbour that names the asymmetry: R3 calls bare `self`, `&self` and
    // `&mut self` all loans, and the bare one already copied. Pinned so the two
    // stay one rule.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun make(self): (i32, i32) { self.pair }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = h.make();
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_returned_own_parameter_still_moves_out_of_a_view_receivers_neighbour() {
    // The `own` elision, unchanged: the caller already gave the value up, so the
    // fluent-builder shape stays free. Proven by the ABSENT `__clone`, since
    // behaviour cannot see the difference.
    let source = r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun through(own h: Holder): Holder { h }
        fun main() {
            let h = Holder { pair = (7, 3) };
            let c = through(h);
            print(c.pair.1);
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "an `own` parameter's return copied:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "3\n");
}

#[test]
fn a_returned_scalar_field_of_a_receiver_needs_no_copy() {
    // The type filter is unchanged: a scalar read IS the copy, so no `__clone`
    // is owed and none is emitted.
    let source = r#"
        import std::print;
        struct Holder { n: i32 }
        impl Holder {
            fun get(&self): i32 { self.n }
        }
        fun main() {
            mut h = Holder { n = 7 };
            let v = h.get();
            h.n = 99;
            print(v);
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(!js.contains("__clone"), "a scalar return copied:\n{js}"),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "7\n");
}

#[test]
fn a_closure_returning_its_own_view_parameters_field_copies() {
    // The closure path, already right before the fix (an unannotated closure
    // parameter is bare) and pinned so the seam's two spellings stay one rule.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun apply(h: &Holder, f: |&Holder| (i32, i32)): (i32, i32) { f(h) }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = apply(&h, |g| g.pair);
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

// ---------------------------------------------------------------------------
// B104 (the classification residual B100 filed). `infer_borrows` recorded `fun
// copy(&self): Holder { self }` as borrowing its receiver — so its result bound
// as a VIEW at every call site — although B100 made that return a COPY. The
// root-set arm whose leaf is a PLACE is the one rule 1's return clause reaches,
// so the two passes now answer the same way about the same seam: where the
// return copies, the place LEFT the loan and the function projects nothing.
// `check_view_escape` still accepts the forwarder, for the same reason (nothing
// escapes when nothing aliased leaves). See proposal/element-clones.md §9.3.
// ---------------------------------------------------------------------------

#[test]
fn a_by_value_forwarders_result_binds_mut() {
    // The filed repro. Both halves in one program: the result is `mut`-bindable
    // (it is a value, not a view), and writing it leaves the receiver alone
    // (B100's copy is what makes the first half true).
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun copy(&self): Holder { self }
        }
        fun main() {
            let h = Holder { pair = (7, 3) };
            mut c = h.copy();
            c.pair.1 = 42;
            print(c.pair.1);
            print(h.pair.1);
        }
        "#,
        "42\n3\n",
    );
}

#[test]
fn a_free_by_value_forwarders_result_binds_mut() {
    // The free-function spelling: nothing here is special to a receiver, and
    // the caller's own value is untouched by the write to the copy.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun copy_of(h: &Holder): Holder { h }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            mut c = copy_of(&h);
            c.pair.1 = 42;
            h.pair.1 = 99;
            print(c.pair.1);
            print(h.pair.1);
        }
        "#,
        "42\n99\n",
    );
}

#[test]
fn a_by_value_forwarder_still_passes_the_escape_check() {
    // The hazard B100 recorded when it refused candidate (c): emptying the
    // root-set makes `check_view_escape` reject the body outright, turning a
    // program that compiles into an error. It must not, and the reason is rule
    // 3's own: the return hands back a value, so no view escapes.
    assert_compiles(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun copy(&self): Holder { self }
        }
        fun main() {
            let h = Holder { pair = (7, 3) };
            print(h.copy().pair.1);
        }
        "#,
    );
}

#[test]
fn a_view_returning_forwarders_result_is_still_a_view() {
    // The other direction, one word apart: the same body under a `&mut Holder`
    // return is a projection, its result is a view, and a view cannot be `mut`.
    assert_fails_with(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun me(&mut self): &mut Holder borrows self { self }
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            mut w = h.me();
            print(w.pair.1);
        }
        "#,
        "cannot be `mut`",
    );
}

#[test]
fn a_view_of_a_local_still_cannot_escape_a_by_value_return() {
    // The escape the new clause must not swallow: rule 1 leaves a place rooted
    // at a LOCAL alone (the frame is a dead owner donating its storage), so no
    // copy converts this one and the view still dangles.
    assert_fails_with(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        impl Holder {
            fun bad(&self): (i32, i32) { let v = &self.pair; v }
        }
        fun main() {
            let h = Holder { pair = (7, 3) };
            print(h.bad().1);
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn a_scalar_view_forwarded_into_a_by_value_return_keeps_its_borrow() {
    // A scalar has no aggregate storage for rule 1 to copy, so the alias
    // survives the return and the (conservative) borrow classification must
    // survive with it — rule 4 still sees a live view into `n`. Pins the gate
    // as CLONEABLE-AGGREGATE-only rather than by-value-only.
    assert_fails_with(
        r#"
        import std::print;
        fun same(v: &mut i32): i32 { v }
        fun main() {
            mut n = 5;
            let m = same(&mut n);
            n = 9;
            print(m);
        }
        "#,
        "rule 4",
    );
}

#[test]
fn a_generic_view_forwarded_into_a_by_value_return_keeps_its_borrow() {
    // The generic twin, and why `Type::Generic` is excluded even though rule 1
    // admits it: a `&T` parameter lowers as a `(base, key)` pair at a scalar
    // instantiation, which `__clone` cannot collapse. The alias survives, so
    // the view classification does too.
    assert_fails_with(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun same<T>(v: &T): T { v }
        fun main() {
            let h = Holder { pair = (7, 3) };
            mut c = same(&h);
            print(c.pair.1);
        }
        "#,
        "cannot be `mut`",
    );
}

#[test]
fn a_borrows_call_chain_into_a_by_value_return_keeps_its_borrow() {
    // The Call arm is deliberately ungated: rule 1 never reaches a call leaf
    // (it is not a place), so no copy is inserted and the result really does
    // alias whatever the signature says. Calling it a borrow is what keeps the
    // call site honest about that.
    assert_fails_with(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun peek(h: &Holder): &(i32, i32) borrows h { &h.pair }
        fun get(h: &Holder): (i32, i32) { peek(h) }
        fun main() {
            let h = Holder { pair = (7, 3) };
            mut p = get(&h);
            print(p.1);
        }
        "#,
        "cannot be `mut`",
    );
}

#[test]
fn a_wrapped_view_return_is_still_a_borrow() {
    // The wrapped arm is ungated for the same reason, and its signature could
    // not carry the gate anyway: `Option<&mut Inner>` is not a view return by
    // `returns_view`, which reads the TOP-LEVEL type node.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun inner_mut(&mut self): Option<&mut Inner> { Some(&mut self.inner) }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 1 } };
            match h.inner_mut() {
                Some(let v) => { v.n = 77; }
                None => {}
            }
            print(h.inner.n);
        }
        "#,
        "77\n",
    );
}

#[test]
fn a_resource_forwarded_out_of_a_loan_is_still_refused() {
    // The resource half of the gate is a guard, not a live case: R1 refuses
    // moving a resource out of a loan before the classification is ever
    // consulted. Pinned so the guard's unreachability is a fact, not a hope.
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard {
            fun drop(own self) { print("drop " + self.tag); }
            fun take(&self): Guard { self }
        }
        fun main() {
            let g = Guard { tag = "a" };
            print(g.take().tag);
        }
        "#,
        "cannot move the resource",
    );
}

// ---------------------------------------------------------------------------
// B108 / B109 (B104's bycatch, both of them rule 1's). The return seam reached
// only leaves that are PLACES, so two leaf shapes handed back a caller's
// storage untouched — `&self.inner` (a `&place`, whose `place_root` is `None`)
// and `peek(h)` (a `borrows` call, likewise) — and a THIRD leaf, the scalar
// view, reached the seam but fell out of the copy's type filter and handed back
// its `(base, key)` pair as the value.
//
// One rule for all three: the seam reads THROUGH a view leaf to the storage it
// names (`returned_value_places`, the return twin of B97's
// `capture_subject_places`), and materializes a VALUE there — `__clone` for a
// cloneable aggregate, the READ for a scalar (a scalar's copy IS its read,
// B81). A RESOURCE crosses out of the loan instead, which the move scan judges
// exactly as it judges the bare place twin. The borrow CLASSIFICATION is
// untouched: gating it too was measured and refused (it turns seven compiling
// shapes into "a view cannot escape its scope" and reddens B104's own chain
// pin). See proposal/element-clones.md §11.
// ---------------------------------------------------------------------------

#[test]
fn a_scalar_view_forwarded_into_a_by_value_return_hands_back_the_value() {
    // B108's filed repro. Printed `[ [ 5 ], 0 ]` — the view's runtime pair, not
    // the i32 the signature promises. The copy machinery is aggregate-shaped
    // and `__clone` cannot collapse a pair; the crossing emits the read.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun same(v: &mut i32): i32 { v }
        fun main() {
            mut n = 5;
            let m = same(&mut n);
            print(m);
        }
        "#,
        "5\n",
    );
}

#[test]
fn a_generic_view_forwarded_into_a_by_value_return_reads_at_a_scalar() {
    // B108's second shape, and the reason the read cannot be decided where the
    // copy is: a generic `&T` is a `(base, key)` pair at exactly its scalar
    // instantiations and the aggregate's own reference everywhere else, which
    // is abstract in the analyzer. Printed `[ [ 5 ], 0 ]` — and `__clone`,
    // which rule 1 does insert here (`Type::Generic` is admitted), deep-COPIED
    // the pair rather than collapsing it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun same<T>(v: &T): T { v }
        fun main() {
            mut n = 5;
            let m = same(&n);
            print(m);
        }
        "#,
        "5\n",
    );
}

#[test]
fn a_scalar_reference_leaf_in_a_by_value_return_reads_the_place() {
    // The `&place` spelling of the same crossing, which is B108 and B109 at
    // once: `&self.n` lowers to the pair `[self, 0]`, and by-value out it must
    // be the field READ. Printed `[ [ 99 ], 0 ]` — the pair, and a LIVE one, so
    // the later write showed through it too.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { n: i32 }
        impl Holder {
            fun grab(&self): i32 { &self.n }
        }
        fun main() {
            mut h = Holder { n = 3 };
            let v = h.grab();
            h.n = 99;
            print(v);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_scalar_borrows_call_leaf_in_a_by_value_return_reads_the_place() {
    // The third spelling: a `borrows` call returning `&i32`, one indirection
    // over. Same pair, same live alias, same read.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { n: i32 }
        fun peek(h: &Holder): &i32 borrows h { &h.n }
        fun get(h: &Holder): i32 { peek(h) }
        fun main() {
            mut h = Holder { n = 3 };
            let v = get(&h);
            h.n = 99;
            print(v);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_reference_leaf_in_a_by_value_return_copies() {
    // B109's filed repro. `&self.inner` is not a place, so
    // `compute_return_clone_sites` never saw it and the caller's result WAS the
    // receiver's field. Printed 99.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun grab(&self): Inner { &self.inner }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let i = h.grab();
            h.inner.n = 99;
            print(i.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_borrows_call_leaf_in_a_by_value_return_copies() {
    // B109's second shape, one indirection over: the tail is a call, so rule 1
    // had no place to copy and the by-value signature handed back the callee's
    // alias. Printed 99. Which places a call names is read from the callee's
    // `borrows` clause at the call site — B97's answer, asked at a return.
    //
    // The classification stays a VIEW here (B104's ungated Call arm, whose own
    // pin is the `mut` twin of this program): conservative, and the copy is
    // what makes it merely conservative rather than wrong.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun peek(h: &Holder): &(i32, i32) borrows h { &h.pair }
        fun get(h: &Holder): (i32, i32) { peek(h) }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let p = get(&h);
            h.pair.1 = 99;
            print(p.1);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_mut_reference_leaf_in_a_by_value_return_copies() {
    // The `&mut` spelling of B109's first shape — one character apart, same
    // answer, because what decides is the SIGNATURE (B100's finding) and not
    // the leaf's own view-ness.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun grab(&mut self): Inner { &mut self.inner }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let i = h.grab();
            h.inner.n = 99;
            print(i.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_free_parameters_reference_leaf_in_a_by_value_return_copies() {
    // Nothing here is special to a receiver: a free `&` parameter is the same
    // loan, and `place_root` walks to it the same way.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        fun grab_of(h: &Holder): Inner { &h.inner }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let i = grab_of(&h);
            h.inner.n = 99;
            print(i.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_nested_reference_leaf_in_a_by_value_return_copies() {
    // Depth is the place walk's business, not the seam's: `&self.mid.inner`
    // roots at `self` like `&self.inner` does.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Mid { inner: Inner }
        struct Holder { mid: Mid }
        impl Holder {
            fun grab(&self): Inner { &self.mid.inner }
        }
        fun main() {
            mut h = Holder { mid = Mid { inner = Inner { n = 3 } } };
            let i = h.grab();
            h.mid.inner.n = 99;
            print(i.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_tail_if_arms_reference_leaf_in_a_by_value_return_copies() {
    // The seam is keyed by the LEAF, so a tail `if` copies per arm — the same
    // reason B100 pinned its own conditional shape.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner, other: Inner }
        impl Holder {
            fun grab(&self, first: bool): Inner {
                if first { &self.inner } else { &self.other }
            }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 }, other = Inner { n = 5 } };
            let i = h.grab(true);
            h.inner.n = 99;
            print(i.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_borrows_method_leaf_in_a_by_value_return_copies() {
    // The method spelling of the call shape: the projected argument is the
    // receiver, at position 0 of the call.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun peek(&self): &Inner borrows self { &self.inner }
            fun grab(&self): Inner { self.peek() }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let i = h.grab();
            h.inner.n = 99;
            print(i.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_borrows_call_chain_leaf_in_a_by_value_return_copies() {
    // The CHAIN, which is what makes the read-through recursive: the outer
    // call's projected argument is itself a call, and only one more step
    // reaches a place at all. Without the recursion this hands back the alias.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Mid { inner: Inner }
        struct Outer { mid: Mid }
        impl Mid {
            fun slot(&mut self): &mut Inner borrows self { &mut self.inner }
        }
        impl Outer {
            fun mid_mut(&mut self): &mut Mid borrows self { &mut self.mid }
        }
        fun grab(o: &mut Outer): Inner { o.mid_mut().slot() }
        fun main() {
            mut o = Outer { mid = Mid { inner = Inner { n = 3 } } };
            let i = grab(&mut o);
            o.mid.inner.n = 99;
            print(i.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_borrows_call_on_a_local_in_a_by_value_return_copies_nothing() {
    // The elision the new arms must not eat, and it is B100's own: the frame is
    // a dead owner at the return, so a projection of a LOCAL donates its
    // storage. Reading through the call reaches `h`, a local, and stops.
    let source = r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        fun peek(h: &Holder): &Inner borrows h { &h.inner }
        fun make(): Inner { let h = Holder { inner = Inner { n = 3 } }; peek(&h) }
        fun main() { print(make().n); }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "a dead owner's donation copied:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "3\n");
}

#[test]
fn an_owned_call_leaf_in_a_by_value_return_copies_nothing() {
    // The other half of "a call owns its result": a callee with no `borrows`
    // clause projects nothing, so reading through it names no place and the
    // seam has nothing to copy.
    let source = r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        fun fresh(): Inner { Inner { n = 3 } }
        fun grab(h: &Holder): Inner { fresh() }
        fun main() {
            let h = Holder { inner = Inner { n = 1 } };
            print(grab(&h).n);
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "an owned call result copied:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "3\n");
}

#[test]
fn a_reference_leaf_in_a_view_return_still_aliases() {
    // Rule 3's sanctioned projection, unchanged: the signature returns a view,
    // so nothing crosses and `&mut self.inner` stays the alias it is for.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun peek(&mut self): &mut Inner borrows self { &mut self.inner }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            h.peek().n = 99;
            print(h.inner.n);
        }
        "#,
        "99\n",
    );
}

#[test]
fn a_reference_leaf_handing_back_a_resource_is_refused() {
    // R1: a resource cannot copy, so the crossing is a MOVE out of the loan —
    // and `&self.g` is `self.g` with a `&` in front of it, which the move scan
    // already refuses as a partial move. Before the fix this compiled and
    // printed the tag with no `drop` at all: the resource left the loan
    // uncopied AND undestroyed.
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        impl Holder {
            fun take(&self): Guard { &self.g }
        }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(h.take().tag);
        }
        "#,
        "cannot move a resource field out of a live aggregate",
    );
}

#[test]
fn a_borrows_call_leaf_handing_back_a_resource_is_refused() {
    // The call spelling of the same crossing: the place it names is the loaned
    // parameter itself, so the diagnostic is R3's — the same one the bare
    // forwarder `fun take(&self): Guard { self }` already gets.
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        fun peek(h: &Holder): &Guard borrows h { &h.g }
        fun take(h: &Holder): Guard { peek(h) }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(take(&h).tag);
        }
        "#,
        "cannot move the resource `h` out of this function",
    );
}

#[test]
fn a_reference_leaf_loaning_a_resource_out_of_a_view_return_is_still_allowed() {
    // And the CROSSING is what decides, not the reference: under a view return
    // the very same `&self.g` is rule 3's projection, moves nothing, and stays
    // legal — the two programs differ only in the return type. (The owner's
    // scope-end teardown is `destruction.md`'s subject, not this seam's.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        impl Holder {
            fun peek(&self): &Guard borrows self { &self.g }
        }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(h.peek().tag);
        }
        "#,
        "a\n",
    );
}

// B116 — the `ret` spelling of a return position gets the tail's analysis.
//
// `check_view_escape` read `Expr::FunctionReturn` as an unconditional escape and
// exempted only `function.body.1`, so `ret &self.inner;` was refused while the
// tail spelling one line away compiled (element-clones.md §11.6). A `ret` is a
// return position exactly like the tail (`ret-checking.md`) and rule 1's return
// clause already treated it as one — `return_sites` carries both — so the two
// spellings disagreed about a value the compiler had already decided to copy.
//
// Every pin below is the SAME program in both spellings: a conditional early
// `ret` whose fall-through tail is the same expression. That shape is what
// isolates the escape question from `ret`'s own early-return-only rule — a body
// that ENDS in `ret x;` is separately a type error ("Expected Inner, but got
// void"), which is why B116's filed repro could not be the probe.

#[test]
fn b116_the_ret_spelling_of_a_reference_leaf_agrees_with_the_tail() {
    // B109's first shape, both spellings in one function. The `ret` was refused;
    // the tail compiled and copied. Both now emit the copy, and the same one.
    let javascript = compile(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun grab(&self, flag: bool): Inner {
                if flag { ret &self.inner; }
                &self.inner
            }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let early = h.grab(true);
            let tail = h.grab(false);
            h.inner.n = 99;
            print(early.n);
            print(tail.n);
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "function grab(self, flag) {\n\
             \tif (flag) {\n\t\treturn __clone(self[0]);\n\t}\n\
             \treturn __clone(self[0]);\n}"
        ),
        "both spellings should emit the same copy, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "3\n3\n",
        "neither spelling may hand back the receiver's field"
    );
}

#[test]
fn b116_the_ret_spelling_of_a_scalar_view_reads_the_place() {
    // B108's shape: a scalar has no aggregate to clone, so its copy is its READ
    // (`v[0][v[1]]`). The `ret` gets the same read, not a refusal and not the
    // runtime pair.
    let javascript = compile(
        r#"
        import std::print;
        fun same(v: &mut i32, flag: bool): i32 {
            if flag { ret v; }
            v
        }
        fun main() { mut n = 5; print(same(&mut n, true)); print(same(&mut n, false)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "function same(v, flag) {\n\
             \tif (flag) {\n\t\treturn v[0][v[1]];\n\t}\n\
             \treturn v[0][v[1]];\n}"
        ),
        "both spellings should read the scalar place, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "5\n5\n",
        "neither spelling may hand back the view pair"
    );
}

#[test]
fn b116_the_ret_spelling_of_a_borrows_call_leaf_copies() {
    // B109's second shape: the leaf is a CALL, so the places it hands back are
    // read from the callee's `borrows` clause. Same answer at the `ret`.
    let javascript = compile(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun peek(h: &Holder): &(i32, i32) borrows h { &h.pair }
        fun get(h: &Holder, flag: bool): (i32, i32) {
            if flag { ret peek(h); }
            peek(h)
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let early = get(&h, true);
            h.pair.1 = 99;
            print(early.1);
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "\tif (flag) {\n\t\treturn __clone(peek(h2));\n\t}\n\treturn __clone(peek(h2));"
        ),
        "both spellings should copy the call's projection, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "3\n",
        "neither spelling may hand back the callee's alias"
    );
}

#[test]
fn b116_the_ret_spelling_of_a_resource_reference_leaf_is_refused() {
    // A resource cannot copy (R1), so the crossing is a MOVE and the move scan
    // refuses it — and it must refuse the `ret` for the SAME reason and with the
    // same words, not because the escape check happened to reject the spelling.
    // Lifting the escape check without telling the crossing about `ret` would
    // have compiled this and leaked the guard uncopied AND undestroyed, which is
    // exactly the bug B109 shipped to close.
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        impl Holder {
            fun take(&self, flag: bool): Guard {
                if flag { ret &self.g; }
                &self.g
            }
        }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(h.take(true).tag);
        }
        "#,
        "cannot move a resource field out of a live aggregate",
    );
}

#[test]
fn b116_the_ret_spelling_of_a_resource_borrows_call_is_refused() {
    // The call spelling of the same crossing, where the place named is the
    // loaned parameter itself — R3's diagnostic, at the `ret` as at the tail.
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        fun peek(h: &Holder): &Guard borrows h { &h.g }
        fun take(h: &Holder, flag: bool): Guard {
            if flag { ret peek(h); }
            peek(h)
        }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(take(&h, true).tag);
        }
        "#,
        "cannot move the resource `h` out of this function",
    );
}

#[test]
fn b116_the_ret_spelling_of_a_view_of_a_local_still_cannot_escape() {
    // The half that must NOT move. Rule 1's return clause leaves a LOCAL alone —
    // the frame is a dead owner donating its storage — so no copy happens and
    // the view really would dangle. Agreement means agreeing on the refusals
    // too: this is refused in both spellings, as the tail always was.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun grab(flag: bool): Inner {
            let local = Inner { n = 3 };
            if flag { ret &local; }
            &local
        }
        fun main() { print(grab(true).n); }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b116_the_ret_spelling_of_a_borrows_projection_is_sanctioned() {
    // The escape check's OTHER exemption, which the `ret` also never got: a
    // `borrows` function may hand back a view of a view parameter, because the
    // caller's argument outlives the call. Rule 3's sanctioned case, now
    // reachable through either spelling — and it emits the projection, not a
    // copy, in both.
    let javascript = compile(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        fun view_of(h: &Holder, flag: bool): &Inner borrows h {
            if flag { ret &h.inner; }
            &h.inner
        }
        fun main() { let h = Holder { inner = Inner { n = 3 } }; print(view_of(&h, true).n); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("\tif (flag) {\n\t\treturn h2[0];\n\t}\n\treturn h2[0];"),
        "a sanctioned projection must not gain a copy, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "3\n",
        "the projection should reach the field"
    );
}

#[test]
fn b116_a_ret_only_resource_crossing_is_named_by_the_move_scan() {
    // The crossing half of B116, isolated: the resource leaves through the
    // `ret` and the TAIL hands back an owned value, so the tail's own crossing
    // says nothing about it. `compute_return_value_crossings` walked
    // `function.body.1` alone, so the `ret`'s `&self.g` named no place and R1
    // never saw the move. It is joined to the return positions now, and the
    // move scan answers with the bare twin's diagnostic.
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        impl Holder {
            fun take(&self, flag: bool): Guard {
                if flag { ret &self.g; }
                Guard { tag = "fresh" }
            }
        }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(h.take(true).tag);
        }
        "#,
        "cannot move a resource field out of a live aggregate",
    );
}

#[test]
fn b122_a_ret_beside_an_owned_tail_agrees_with_the_conditional_tail() {
    // Found closing B116 and closed by B122: the two spellings here were
    // examined by different questions. The tail loop asked
    // `escapes_as_view(function.body.1)` of the WHOLE body, and an `if` with
    // one owned arm is not a view expression, so it was never asked at all;
    // the `ret` arm asked the leaf, which is. `infer_borrows` walked only the
    // tail, so the function's root-set stayed empty when a view reached the
    // caller only through a `ret` — and rule 1's return clause copied it
    // regardless, which is what made the refusal a false positive rather than
    // a disagreement about the rule. `element-clones.md` §13 closes the
    // measurement §11.3 candidate (d) deferred.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun early(&self, flag: bool): Inner {
                if flag { ret &self.inner; }
                Inner { n = 0 }
            }
            fun conditional(&self, flag: bool): Inner {
                if flag { Inner { n = 0 } } else { &self.inner }
            }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let early = h.early(true);
            let tail = h.conditional(false);
            h.inner.n = 99;
            print(early.n);
            print(tail.n);
        }
        "#,
        "3\n3\n",
    );
}

#[test]
fn b122_a_conditional_tail_arm_may_not_escape_a_view_of_a_local() {
    // The other side of the same hole, and the one that mattered more: rule 3
    // exists to refuse exactly this, and a second owned arm hid it — the whole
    // body's "is this a view expression" question is `false` for an `if` with
    // one owned arm, so the view arm was never asked at all. The `ret`
    // spelling was already refused, so the two spellings used to disagree in
    // the direction that let code through. (Benign as emitted — the frame is
    // dead and nothing else holds the storage — but it was the rule not being
    // applied, not the rule deciding.) `check_view_escape` now asks each
    // return LEAF (`collect_tail_leaves`), so this arm is examined on its own.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun grab(flag: bool): Inner {
            let local = Inner { n = 3 };
            if flag { Inner { n = 0 } } else { &local }
        }
        fun main() { print(grab(false).n); }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b122_a_conditional_tail_arm_order_does_not_matter() {
    // The mirror of the previous pin: the view-of-a-local arm FIRST, the owned
    // arm second. `collect_tail_leaves` walks both arms of an `if`/`else`
    // regardless of order, so which side hides which is not the question —
    // every leaf gets asked.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun grab(flag: bool): Inner {
            let local = Inner { n = 3 };
            if flag { &local } else { Inner { n = 0 } }
        }
        fun main() { print(grab(false).n); }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b122_a_nested_conditional_arm_may_not_escape_a_view_of_a_local() {
    // `collect_tail_leaves` recurses into a nested `if`'s own branches
    // (`collect_tail_leaves_if`), so a view of a local buried two levels deep
    // is still a leaf the walk reaches, not just the immediate arms of the
    // outermost `if`.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun grab(flag: bool, other: bool): Inner {
            let local = Inner { n = 3 };
            if flag {
                if other { Inner { n = 1 } } else { Inner { n = 2 } }
            } else {
                &local
            }
        }
        fun main() { print(grab(false, false).n); }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b122_a_match_leg_may_not_escape_a_view_of_a_local() {
    // The same walk over a `match` tail (`collect_tail_leaves`'s other arm):
    // one leg owned, one leg a view of a local, and the local leg is still
    // examined on its own regardless of which leg the whole match's TYPE
    // would suggest is representative.
    assert_fails_with(
        r#"
        import std::print;
        enum Choice { A, B }
        struct Inner { n: i32 }
        fun grab(choice: Choice): Inner {
            let local = Inner { n = 3 };
            match choice {
                Choice::A => Inner { n = 0 },
                Choice::B => &local,
            }
        }
        fun main() { print(grab(Choice::A).n); }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b122_a_mixed_leaf_return_refuses_only_the_local_view_leaf() {
    // The multi-leaf mix the leaf-wise walk exists to get right: one arm
    // projects a PARAMETER (sound — the caller's argument outlives the call)
    // and the other a LOCAL (unsound). Leaf-wise, the two are asked
    // separately and answered separately — exactly one diagnostic, and its
    // span is the local arm, not the parameter arm and not the whole `if`.
    let source = r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        fun grab(h: &Holder, flag: bool): Inner {
            let local = Inner { n = 3 };
            if flag { &h.inner } else { &local }
        }
        fun main() {
            let h = Holder { inner = Inner { n = 9 } };
            print(grab(&h, true).n);
        }
        "#;
    assert_fails_once_with(source, "a view cannot escape its scope");
    assert_fails_spanning(source, "&local", "a view cannot escape its scope");
}

#[test]
fn a_closures_ret_still_cannot_hand_back_a_view() {
    // The boundary of B116's lift. A closure's rets check against its INFERRED
    // tail type and never enter `return_sites`, so they get no exemption — which
    // is right, because a closure may not declare `borrows` and cannot return a
    // view at all (`compute_return_clone_sites` relies on exactly that).
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        fun main() {
            let h = Holder { inner = Inner { n = 3 } };
            let f = |flag: bool| { if flag { ret &h.inner; } &h.inner };
            print(f(true).n);
        }
        "#,
        "a view cannot escape its scope",
    );
}

// --- B134: `return_sites` completes — the tail and each value-carrying `ret`
// of EVERY bodied function, annotated or not. B116 built the join for
// declared-return functions; B126 typed an unannotated function's `ret`s, so
// the seam readers (`infer_borrows`, the crossing scan, `check_view_escape`,
// the return clone sites) must see those positions too. Every pin below is a
// B116/B122 shape with the return annotation REMOVED: the two spellings of
// one return — and the two spellings of one signature — must answer alike.

#[test]
fn b134_the_unannotated_ret_spelling_of_a_reference_leaf_copies() {
    // B116's first shape without the `: Inner`. The `ret` was refused with
    // the generic "a view cannot escape its scope" (the raw FunctionReturn
    // scan — the seams never saw an unannotated `ret`) while the tail
    // ALIASED (below). Both spellings now emit the same copy the annotated
    // twin has always emitted.
    let javascript = compile(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun grab(&self, flag: bool) {
                if flag { ret &self.inner; }
                &self.inner
            }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let early = h.grab(true);
            let tail = h.grab(false);
            h.inner.n = 99;
            print(early.n);
            print(tail.n);
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "function grab(self, flag) {\n\
             \tif (flag) {\n\t\treturn __clone(self[0]);\n\t}\n\
             \treturn __clone(self[0]);\n}"
        ),
        "both unannotated spellings should emit the same copy, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "3\n3\n",
        "neither spelling may hand back the receiver's field"
    );
}

#[test]
fn b134_an_unannotated_tail_of_a_loaned_place_copies() {
    // The tail half of the same gap, and the sharpest tooth: with no
    // annotation the tail was not a clone seam at all (`return_sites` was
    // the clone-site pass's only function source), so `fun grab(&self) {
    // self.inner }` handed back the receiver's LIVE storage — this program
    // printed 99 where its annotated twin printed 3.
    let javascript = compile(
        r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        impl Holder {
            fun grab(&self) {
                self.inner
            }
        }
        fun main() {
            mut h = Holder { inner = Inner { n = 3 } };
            let got = h.grab();
            h.inner.n = 99;
            print(got.n);
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("return __clone(self[0]);"),
        "the unannotated tail must copy the loaned place, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "3\n",
        "the unannotated tail may not hand back the receiver's field"
    );
}

#[test]
fn b134_the_unannotated_ret_spelling_of_a_scalar_view_reads_the_place() {
    // B108's shape without the `: i32`: a scalar's copy is its read
    // (`v[0][v[1]]`), at the `ret` as at the tail.
    let javascript = compile(
        r#"
        import std::print;
        fun same(v: &mut i32, flag: bool) {
            if flag { ret v; }
            v
        }
        fun main() { mut n = 5; print(same(&mut n, true)); print(same(&mut n, false)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "function same(v, flag) {\n\
             \tif (flag) {\n\t\treturn v[0][v[1]];\n\t}\n\
             \treturn v[0][v[1]];\n}"
        ),
        "both unannotated spellings should read the scalar place, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "5\n5\n",
        "neither spelling may hand back the view pair"
    );
}

#[test]
fn b134_the_unannotated_ret_spelling_of_a_borrows_call_leaf_copies() {
    // The call-leaf shape: the caller is unannotated, the `borrows` callee
    // keeps its declaration (that sanction is the SIGNATURE's, and an
    // unannotated function has none to give). The projection is copied out
    // at both of the caller's return positions.
    let javascript = compile(
        r#"
        import std::print;
        struct Holder { pair: (i32, i32) }
        fun peek(h: &Holder): &(i32, i32) borrows h { &h.pair }
        fun get(h: &Holder, flag: bool) {
            if flag { ret peek(h); }
            peek(h)
        }
        fun main() {
            mut h = Holder { pair = (7, 3) };
            let early = get(&h, true);
            h.pair.1 = 99;
            print(early.1);
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "\tif (flag) {\n\t\treturn __clone(peek(h2));\n\t}\n\treturn __clone(peek(h2));"
        ),
        "both unannotated spellings should copy the call's projection, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "3\n",
        "neither spelling may hand back the callee's alias"
    );
}

#[test]
fn b134_the_unannotated_ret_spelling_of_a_resource_reference_leaf_is_refused() {
    // A resource cannot copy (R1), so the crossing is a MOVE and the move
    // scan refuses it — the same words as the annotated twin
    // (`b116_the_ret_spelling_of_a_resource_reference_leaf_is_refused`),
    // where the unannotated `ret` used to draw the generic escape refusal
    // (the crossing scan never saw it).
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        impl Holder {
            fun take(&self, flag: bool) {
                if flag { ret &self.g; }
                &self.g
            }
        }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(h.take(true).tag);
        }
        "#,
        "cannot move a resource field out of a live aggregate",
    );
}

#[test]
fn b134_an_unannotated_ret_only_resource_crossing_is_named_by_the_move_scan() {
    // The crossing half isolated, as B116 isolated it: the resource leaves
    // only through the `ret` (the tail hands back an owned value), so
    // walking the tail alone says nothing about it.
    assert_fails_with(
        r#"
        import std::print;
        resource struct Guard { tag: str }
        impl Guard { fun drop(own self) { print("drop " + self.tag); } }
        struct Holder { g: Guard }
        impl Holder {
            fun take(&self, flag: bool) {
                if flag { ret &self.g; }
                Guard { tag = "fresh" }
            }
        }
        fun main() {
            let h = Holder { g = Guard { tag = "a" } };
            print(h.take(true).tag);
        }
        "#,
        "cannot move a resource field out of a live aggregate",
    );
}

#[test]
fn b134_the_unannotated_ret_spelling_of_a_view_of_a_local_still_cannot_escape() {
    // The half that must NOT change: a view of a LOCAL dangles whatever the
    // signature says, and the seam walk refuses it in both spellings exactly
    // as the raw scan did.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun grab(flag: bool) {
            let local = Inner { n = 3 };
            if flag { ret &local; }
            &local
        }
        fun main() { print(grab(true).n); }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b123_a_closure_conditional_tail_arm_may_not_escape_a_view_of_a_closure_local() {
    // The un-masking pin (`element-clones.md` §13.4 / backlog B123): B122 gave
    // `check_view_escape`'s FUNCTION seam a leaf walk, but the closure seam
    // kept the old whole-position question — `escapes_as_view(closure.return_)`
    // asks the closure's whole `Block`/`If`, never leaf-wise, and `is_view_expr`
    // matches neither directly. Every existing pin happened to also spell a
    // `ret`, which the per-expr `Expr::FunctionReturn` arm catches unconditionally
    // regardless of this hole — so the blindness was self-masked. Here there is
    // no `ret` anywhere: the view of a closure-local reaches the caller only
    // through one arm of the closure's conditional TAIL, and the whole-block
    // question was `false` for an `if`, so nothing asked the arm that
    // mattered. Wrongly compiled before this fix (verified against the live
    // compiler, planted red); the `ret` spelling of the identical shape was
    // already refused
    // (`b123_a_closure_ret_and_conditional_tail_arm_agree_refusing_a_view_of_a_closure_local`).
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun main() {
            let grab = |flag: bool| {
                let local = Inner { n = 3 };
                if flag { Inner { n = 0 } } else { &local }
            };
            print(grab(false).n);
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b123_a_closure_ret_and_conditional_tail_arm_agree_refusing_a_view_of_a_closure_local() {
    // The REFUSE-direction agreement pin (B116/B122 style): the same shape,
    // spelled the two ways a closure return can be spelled, must answer
    // identically. The `ret` spelling was already refused (the per-expr
    // `Expr::FunctionReturn` arm has no exemption for a closure either), and
    // is not new — what B123 fixes is that the conditional-tail spelling used
    // to disagree, letting the identical view of a closure-local through.
    // Both refuse now.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun main() {
            let via_ret = |flag: bool| {
                let local = Inner { n = 3 };
                if flag { ret &local; }
                Inner { n = 0 }
            };
            print(via_ret(false).n);
        }
        "#,
        "a view cannot escape its scope",
    );
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun main() {
            let via_tail = |flag: bool| {
                let local = Inner { n = 3 };
                if flag { Inner { n = 0 } } else { &local }
            };
            print(via_tail(false).n);
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b123_a_closure_ret_and_conditional_tail_arm_agree_accepting_an_owned_leaf() {
    // The ACCEPT-direction agreement pin. Unlike the function seam (B122's
    // `b122_a_ret_beside_an_owned_tail_agrees_with_the_conditional_tail`), a
    // closure has no exemption at all to accept a SOUND view through — it may
    // not declare `borrows`, so every view leaf is unconditionally an escape
    // (P4c). The accept side that exists is the neutral one: an OWNED leaf,
    // which `escapes_as_view` was never going to flag either way, must still
    // compile through both spellings after the leaf walk — proving the walk
    // widens what gets ASKED, not what gets REFUSED.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun main() {
            let via_ret = |flag: bool| {
                if flag { ret Inner { n = 1 }; }
                Inner { n = 0 }
            };
            let via_tail = |flag: bool| {
                if flag { Inner { n = 1 } } else { Inner { n = 0 } }
            };
            print(via_ret(true).n);
            print(via_tail(false).n);
        }
        "#,
        "1\n0\n",
    );
}

#[test]
fn b123_a_closure_conditional_tail_arm_order_does_not_matter() {
    // The mirror of `b123_a_closure_conditional_tail_arm_may_not_escape_a_view_of_a_closure_local`:
    // the view-of-a-local arm FIRST, the owned arm second. `collect_tail_leaves`
    // walks both arms of an `if`/`else` regardless of order.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun main() {
            let grab = |flag: bool| {
                let local = Inner { n = 3 };
                if flag { &local } else { Inner { n = 0 } }
            };
            print(grab(false).n);
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b123_a_nested_closure_conditional_arm_may_not_escape_a_view_of_a_closure_local() {
    // `collect_tail_leaves_if`'s recursion into a nested `if`'s own branches
    // reaches the closure seam too — a view of a closure-local buried two
    // levels deep is still a leaf the walk finds.
    assert_fails_with(
        r#"
        import std::print;
        struct Inner { n: i32 }
        fun main() {
            let grab = |flag: bool, other: bool| {
                let local = Inner { n = 3 };
                if flag {
                    if other { Inner { n = 1 } } else { Inner { n = 2 } }
                } else {
                    &local
                }
            };
            print(grab(false, false).n);
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b123_a_closure_match_leg_may_not_escape_a_view_of_a_closure_local() {
    // `collect_tail_leaves`'s `match` arm reaches the closure seam too: one
    // leg owned, one leg a view of a closure-local, and the local leg is
    // still examined on its own.
    assert_fails_with(
        r#"
        import std::print;
        enum Choice { A, B }
        struct Inner { n: i32 }
        fun main() {
            let grab = |choice: Choice| {
                let local = Inner { n = 3 };
                match choice {
                    Choice::A => Inner { n = 0 },
                    Choice::B => &local,
                }
            };
            print(grab(Choice::A).n);
        }
        "#,
        "a view cannot escape its scope",
    );
}

#[test]
fn b123_a_mixed_leaf_closure_return_refuses_each_forbidden_view_leaf_separately() {
    // Three arms: one owned, one a view of a CAPTURE (`&h.inner`, the same
    // shape `a_closures_ret_still_cannot_hand_back_a_view` already refuses
    // through `ret`), one a view of a closure-LOCAL. Unlike the function
    // seam's mixed pin (`b122_a_mixed_leaf_return_refuses_only_the_local_view_leaf`,
    // which has one SOUND leaf and refuses exactly once), a closure has no
    // sound view leaf at all — both view arms are forbidden, so the leaf walk
    // must refuse BOTH, separately, each naming its own span rather than
    // collapsing into one diagnostic or naming the enclosing `if`.
    let source = r#"
        import std::print;
        struct Inner { n: i32 }
        struct Holder { inner: Inner }
        fun main() {
            let h = Holder { inner = Inner { n = 9 } };
            let grab = |flag: bool, other: bool| {
                if flag {
                    Inner { n = 0 }
                } else if other {
                    &h.inner
                } else {
                    let local = Inner { n = 3 };
                    &local
                }
            };
            print(grab(false, false).n);
        }
        "#;
    let matching = failure_diagnostics(source)
        .into_iter()
        .filter(|(message, _)| message.contains("a view cannot escape its scope"))
        .count();
    assert_eq!(
        matching, 2,
        "expected exactly two escape diagnostics, one per forbidden view leaf"
    );
    assert_fails_spanning(source, "&h.inner", "a view cannot escape its scope");
    assert_fails_spanning(source, "&local", "a view cannot escape its scope");
}

#[test]
fn a_list_literal_element_copies_its_source_place() {
    // B54: `[xs]` installed the caller's storage as element 0, so growing the
    // result's element grew `xs`. Printed 3.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [1, 2];
            mut ys = [xs];
            ys[0].push(9);
            print(xs.len());
            print(ys[0].len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_tuple_literal_element_copies_its_source_place() {
    // The same seam through a tuple, whose elements store flat.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [1, 2];
            mut pair = (xs, 1);
            pair.0.push(9);
            print(xs.len());
            print(pair.0.len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_struct_literal_field_copies_its_source_place() {
    // Rule 1 names "field initialization" outright; it was not enforced.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { items: List<i32> }
        fun main() {
            mut xs = [1, 2];
            mut holder = Holder { items = xs };
            holder.items.push(9);
            print(xs.len());
            print(holder.items.len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_variant_payload_copies_its_source_place() {
    // A variant constructor has no `Parameter` entries, so the `own`-argument
    // arm never saw it: `Some(xs)` captured `xs`'s storage as the payload.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut xs = [1, 2];
            let held = Some(xs);
            xs.push(9);
            match held {
                Some(let inner) => print(inner.len()),
                None => print(0),
            }
            print(xs.len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_construction_does_not_see_later_writes_to_its_source() {
    // The read direction of the list-literal seam: the source grows, and the
    // already-built list must not.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [1, 2];
            let held = [xs];
            xs.push(9);
            print(held[0].len());
            print(xs.len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_nested_construction_copies_at_every_level() {
    // A struct literal inside a list literal: the copy has to reach the inner
    // field, not just the outer element.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Row { cells: List<i32> }
        fun main() {
            mut cells = [1, 2];
            mut rows = [Row { cells = cells }];
            rows[0].cells.push(9);
            print(cells.len());
            print(rows[0].cells.len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_pushed_place_copies_into_the_receiver() {
    // A20's root: `push` STORES its argument, so the argument is owned by the
    // callee — `own item: T`. Without it `acc.push(xs)` filed the caller's
    // storage into `acc`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [1, 2];
            mut acc: List<List<i32>> = List::new();
            acc.push(xs);
            acc[0].push(9);
            print(xs.len());
            print(acc[0].len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn filter_does_not_share_elements_with_its_receiver() {
    // A20: `filter` pushes the loop element straight through.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs: List<List<i32>> = List::new();
            xs.push([1, 2]);
            mut kept = xs.filter(|c| true);
            kept[0].push(9);
            print(xs[0].len());
            print(kept[0].len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn reverse_does_not_share_elements_with_its_receiver() {
    // A20's record claimed `reverse` "happens not to alias" because it rebuilds
    // through `push`. It does alias: rebuilding through `push` copies the SPINE
    // only, and `self[index]` hands the element over uncopied.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs: List<List<i32>> = List::new();
            xs.push([1, 2]);
            xs.push([3]);
            mut flipped = xs.reverse();
            flipped[1].push(9);
            print(xs[0].len());
            print(flipped[1].len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn sort_by_does_not_share_elements_with_its_receiver() {
    // A20: the intrinsic is `list.slice().sort(cmp)` — a new spine over the
    // same elements.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::compare::Ordering;
        struct Cell { n: i32 }
        fun main() {
            mut cells: List<Cell> = List::new();
            cells.push(Cell { n = 5 });
            mut sorted = cells.sort_by(|a, b| a.n.compare(b.n));
            sorted[0].n = 99;
            print(cells[0].n);
            print(sorted[0].n);
        }
        "#,
        "5\n99\n",
    );
}

#[test]
fn map_does_not_share_elements_with_its_receiver() {
    // A20's fourth method, and the one the STORE rule alone does not reach:
    // `map` pushes `fn(item)`, a call result, which the elision framework
    // assumes is owned. It is not owned when the closure returns a place it
    // borrowed — so the copy lands at the closure's RETURN.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs: List<List<i32>> = List::new();
            xs.push([1, 2]);
            mut mapped = xs.map(|c| c);
            mapped[0].push(9);
            print(xs[0].len());
            print(mapped[0].len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_list_method_chain_does_not_share_elements() {
    // The composed case: every hop of `map(...).filter(...)` has to hold.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs: List<List<i32>> = List::new();
            xs.push([1, 2]);
            mut out = xs.map(|c| c).filter(|c| true);
            out[0].push(9);
            print(xs[0].len());
            print(out[0].len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_returned_parameter_place_does_not_alias_the_caller() {
    // "Calls own their result" is the assumption every copy elision rests on
    // (`compute_clone_sites`' own doc comment). A function returning a
    // by-value parameter broke it outright: the caller's binding skipped its
    // copy because the initializer was a call, and the call handed back the
    // caller's own storage.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun identity(c: List<i32>): List<i32> { c }
        fun main() {
            mut xs = [1, 2];
            mut got = identity(xs);
            got.push(9);
            print(xs.len());
            print(got.len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_returned_field_of_a_parameter_does_not_alias_the_caller() {
    // The projecting getter — the shape that makes `map`'s closure leak.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { items: List<i32> }
        fun items_of(holder: Holder): List<i32> { holder.items }
        fun main() {
            mut holder = Holder { items = [1, 2] };
            mut got = items_of(holder);
            got.push(9);
            print(holder.items.len());
            print(got.len());
        }
        "#,
        "2\n3\n",
    );
}

#[test]
fn a_returned_local_still_moves() {
    // The elision the return rule must NOT eat: a function's own local is a
    // dead owner at the tail, so it donates its storage rather than copying.
    // Behaviour is identical either way — `copy-elision.js`/`list-methods.js`
    // pin the absence of `__clone` in bytes — so this only guards the output.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun build(): List<List<i32>> {
            mut result: List<List<i32>> = List::new();
            result.push([1, 2]);
            result
        }
        fun main() {
            mut built = build();
            built[0].push(9);
            print(built[0].len());
        }
        "#,
        "3\n",
    );
}

// --- B64: a CLOSURE returns from a frame it does not own ----------------------
//
// The return rule's two free cases — a local (a dead owner at the tail) and an
// `own` parameter (the callee's own storage) — both rest on "the returning frame
// owns this". Inside a closure that is false for anything the closure did not
// declare: the capture's frame does not die at the closure's return, and a
// closure runs many times where a body runs once. `element-clones.md` §7 filed
// the local half; the `own`-parameter half fell out of the same walk.

#[test]
fn a_closure_returning_a_captured_local_does_not_alias_it() {
    // §7's repro. `|| xs` handed out `xs`'s live storage, so pushing to the
    // result grew `xs` — the same leak `fun identity(c) { c }` had before the
    // parameter half of this rule, spelled inline.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = [ 1, 2 ];
            let get = || xs;
            mut got = get();
            got.push(9);
            print(got.len());
            print(xs.len());
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_closure_returning_a_captured_own_parameter_does_not_alias_it() {
    // The half §7 did not name, and the reason the fix is not "captures behave
    // like bare parameters": an `own` parameter IS free to return directly (the
    // callee owns it — that is the fluent-builder elision), but a closure over
    // it hands out the SAME storage on every call. Two calls, two independent
    // lists. The bare-parameter twin already worked and is the control.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun make_bare(items: List<i32>): || List<i32> { || items }
        fun make_own(own items: List<i32>): || List<i32> { || items }
        fun main() {
            let bare = make_bare([ 1, 2 ]);
            mut first = bare();
            first.push(9);
            print(bare().len());
            let owned = make_own([ 1, 2 ]);
            mut second = owned();
            second.push(9);
            print(owned().len());
        }
        "#,
        "2\n2\n",
    );
}

#[test]
fn a_closure_returning_its_own_local_still_moves() {
    // The elision the capture rule must NOT eat: a closure's own local is a
    // dead owner at ITS tail, exactly like a function's. Behaviour cannot see
    // the difference — a copy would be correct too — so the proof is the
    // emitted bytes, and the program below has no other `__clone` site.
    let source = r#"
        import std::print;
        fun main() {
            let build = || {
                mut result = [ 1, 2 ];
                result.push(3);
                result
            };
            mut first = build();
            first.push(9);
            print(first.len());
            print(build().len());
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            !js.contains("__clone"),
            "the closure's own local no longer donates:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "4\n3\n");
}

#[test]
fn a_closure_returning_a_captured_field_does_not_alias_it() {
    // Keyed by the place's ROOT, so a projection out of a captured aggregate
    // copies on the same rule — the `map`-closure shape, one frame in.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Holder { items: List<i32> }
        fun main() {
            mut holder = Holder { items = [ 1, 2 ] };
            let items_of = || holder.items;
            mut got = items_of();
            got.push(9);
            print(got.len());
            print(holder.items.len());
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_closures_own_is_capture_is_not_a_resource_capture() {
    // Found on the way, pre-existing on v0.25.0 and fixed by the same walk: R9
    // built its declared-inside set without the bindings an `is` pattern
    // introduces, so a closure testing its OWN parameter reported the capture
    // it had just bound as a resource captured from the frame around it. The
    // `match` twin was always fine — only the `is` arm was missing.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Db { handle: i32 }
        fun main() {
            let read = |o: Option<Db>| {
                if o is Some(let d) { d.handle } else { 0 }
            };
            print(read(Some(Db { handle = 4 })));
        }
        "#,
        "4\n",
    );
}

#[test]
fn a_guard_that_lifts_emits_its_temporary() {
    // B59, the `?` shape: the lift compiles to a temp plus an `if` over the
    // container's variant, all of which the else-if chain used to drop.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let held: Option<List<i32>> = Some([1, 2, 3]);
            match held {
                Some(let inner) if (held?.len()).unwrap_or(0) > 2 => print(inner.len()),
                _ => print(0),
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_guard_that_matches_emits_its_temporary() {
    // B59, the nested-`match` shape: an inner match is a subject temp, a result
    // temp and an if-chain — three statements with nowhere to go.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let held: Option<List<i32>> = Some([1, 2, 3]);
            match held {
                Some(let inner) if match inner.len() { 0 => false, _ => true } => {
                    print(inner.len());
                }
                _ => print(0),
            }
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_later_guarded_leg_gets_its_own_slot() {
    // B59, ordering: the leg that needs the slot is not the first, so its
    // statements have to run only after the earlier leg has declined — a slot
    // hoisted to the top of the match would pop before the first guard is
    // even asked.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut held: Option<List<i32>> = Some([1, 2, 3]);
            match held {
                Some(let inner) if inner.len() > 5 => print(1),
                Some(mut inner) if inner.pop() is Some(let last) => {
                    print(last);
                    print(inner.len());
                }
                _ => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn a_guard_that_reads_a_copied_capture_reads_the_copy() {
    // B59: the copy a capture owes is declared ahead of a guard that READS it —
    // the guard and the body must see the same binding. `inner` is returned (the
    // value seam), so it copies; the guard's `len` is the copy's.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun keep(held: Option<List<i32>>): List<i32> {
            match held {
                Some(let inner) if inner.len() > 1 => inner,
                _ => List::new(),
            }
        }
        fun main() {
            let held: Option<List<i32>> = Some([1, 2]);
            mut got = keep(held);
            got.push(9);
            print(got.len());
            match held {
                Some(let inner) => print(inner.len()),
                None => print(0),
            }
        }
        "#,
        "3\n2\n",
    );
}

#[test]
fn trait_conformance_ignores_parameter_mut() {
    // `mut` is the impl's local business — a trait signature without it is
    // satisfied by an impl with it (and the receiver likewise).
    assert_compiles_and_runs(
        r#"
        import std::print;
        trait Doubler { fun doubled(self, x: i32): i32; }
        struct Twice {}
        impl Twice with Doubler {
            fun doubled(mut self, mut x: i32): i32 { x = x * 2; x }
        }
        fun main() { print(Twice {}.doubled(21)); }
        "#,
        "42\n",
    );
}

#[test]
fn shared_write_is_a_view_not_a_value() {
    // `write()` returns a view (`&mut T`), so binding its result to a value slot
    // is rejected (transparent references R1) — use `read()` or `*`.
    assert_fails(
        r#"
        import std::shared::Shared;
        fun main() { let c = Shared::new(5); let x: i32 = c.write(); }
        "#,
    );
}
