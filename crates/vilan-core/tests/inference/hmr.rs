//! A13 HMR: identity, fingerprints, adopt/expose emission, and the
//! `dev::stash` / `dev::take` transfer bound.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- A13 S2a: HMR identity, fingerprints, and adopt/expose emission -----------

#[test]
fn hmr_value_binding_wraps_initializer_and_exposes_it() {
    // A plain-data binding adopts the value itself: `__hmr_adopt(key, fp, () =>
    // <init>)`, and exposes it with a `() => <name>` getter at the module tail.
    let js = compile_hmr(
        r#"
        import std::io::print;
        mut count = 0;
        fun main() { count = count + 1; print(count); }
        "#,
    );
    assert!(
        js.contains(r#"__hmr_adopt("pkg::count", "#),
        "value binding should wrap with __hmr_adopt: {js}"
    );
    assert!(
        js.contains(r#"__hmr_expose("pkg::count", "#),
        "value binding should be exposed: {js}"
    );
    // The adopt thunk returns the original initializer, the getter reads the live
    // binding.
    assert!(
        js.contains("return 0;"),
        "thunk returns the initializer: {js}"
    );
    assert!(
        js.contains("return count;"),
        "getter reads the binding: {js}"
    );
}

#[test]
fn hmr_mut_binding_is_let_and_immutable_binding_is_const() {
    // The declaration keyword is preserved through the wrap: a `mut` binding stays
    // JS `let`, an immutable one stays `const`.
    let js = compile_hmr(
        r#"
        import std::io::print;
        mut counter = 0;
        let label = "hi";
        fun main() { counter = counter + 1; print(label); }
        "#,
    );
    assert!(
        js.contains(r#"let counter = __hmr_adopt("pkg::counter", "#),
        "mut binding is a JS `let`: {js}"
    );
    assert!(
        js.contains(r#"const label = __hmr_adopt("pkg::label", "#),
        "immutable binding is a JS `const`: {js}"
    );
}

#[test]
fn hmr_signal_binding_uses_payload_form() {
    // A `SignalCell<T>` adopts through the payload form, and its getter reads the value
    // cell (`[0].v`) so only the value crosses — old subscribers die.
    let js = compile_hmr(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        let ticker = Signal::new(0);
        fun main() { print(ticker.get()); }
        "#,
    );
    assert!(
        js.contains(r#"__hmr_adopt_signal("pkg::ticker", "#),
        "signal binding uses the signal payload adopt: {js}"
    );
    assert!(
        js.contains("return ticker[0].v;"),
        "signal getter reads the value cell: {js}"
    );
}

#[test]
fn hmr_shared_binding_uses_payload_form() {
    // A `Shared<T>` adopts through the payload form; its getter reads the cell slot
    // (`.v`).
    let js = compile_hmr(
        r#"
        import std::io::print;
        import std::shared::Shared;
        let cell = Shared::new(0);
        fun main() { print(cell.read()); }
        "#,
    );
    assert!(
        js.contains(r#"__hmr_adopt_shared("pkg::cell", "#),
        "shared binding uses the shared payload adopt: {js}"
    );
    assert!(
        js.contains("return cell.v;"),
        "shared getter reads the cell slot: {js}"
    );
}

#[test]
fn hmr_excluded_binding_is_emitted_unwrapped_and_unexposed() {
    // A binding whose type carries code (a struct with a closure field) is not
    // transferable: it emits its declaration exactly as usual — no adopt wrap — and
    // is never exposed.
    let js = compile_hmr(
        r#"
        import std::io::print;
        struct Holder { action: || i32 }
        let holder = Holder { action = || 42 };
        fun main() { print((holder.action)()); }
        "#,
    );
    assert!(
        !js.contains("pkg::holder"),
        "an excluded binding is neither adopted nor exposed: {js}"
    );
    assert!(
        js.contains("const holder = ") && !js.contains("const holder = __hmr"),
        "the excluded binding still emits its plain, unwrapped declaration: {js}"
    );
}

#[test]
fn hmr_nested_module_binding_key_carries_the_module_path() {
    // A binding declared inside a `mod` is keyed `pkg::<module>::<name>` — from its
    // DECLARING scope, so a `use` re-export cannot relabel its home.
    let js = compile_hmr(
        r#"
        import std::io::print;
        mod inner { export let greeting = "hey"; }
        use inner::greeting;
        fun main() { print(greeting); }
        "#,
    );
    assert!(
        js.contains(r#"__hmr_adopt("pkg::inner::greeting", "#),
        "nested binding carries its module path: {js}"
    );
    assert!(
        js.contains(r#"__hmr_expose("pkg::inner::greeting", "#),
        "nested binding is exposed under its module path: {js}"
    );
}

#[test]
fn hmr_fingerprint_is_stable_for_equal_types_and_differs_on_a_type_change() {
    // Two bindings of the same type share a fingerprint; changing a struct field's
    // type flips it (so the swap falls back to fresh init instead of adopting a
    // stale shape). The fingerprint is over the structural type, not the value.
    let same = compile_hmr(
        r#"
        import std::io::print;
        let a = 1;
        let b = 2;
        fun main() { print(a); print(b); }
        "#,
    );
    let fp_a = hmr_fingerprint(&same, "pkg::a").expect("fp a");
    let fp_b = hmr_fingerprint(&same, "pkg::b").expect("fp b");
    assert_eq!(fp_a, fp_b, "same type (i32) hashes the same");

    let point_i32 = compile_hmr(
        r#"
        import std::io::print;
        struct Point { x: i32 }
        let p = Point { x = 1 };
        fun main() { print(p.x); }
        "#,
    );
    let point_str = compile_hmr(
        r#"
        import std::io::print;
        struct Point { x: str }
        let p = Point { x = "a" };
        fun main() { print(p.x); }
        "#,
    );
    let fp_int = hmr_fingerprint(&point_i32, "pkg::p").expect("fp int point");
    let fp_str = hmr_fingerprint(&point_str, "pkg::p").expect("fp str point");
    assert_ne!(
        fp_int, fp_str,
        "a changed field type must change the fingerprint"
    );
}

#[test]
fn hmr_function_local_is_not_wrapped_or_exposed() {
    // A function-local `let` is function-minted state, NOT a module-level binding
    // (hmr.md §3: it must reset on a swap). Its declaring scope is a function body,
    // never a root / `mod` body, so it is never classified — no adopt, no expose.
    let js = compile_hmr(
        r#"
        import std::io::print;
        fun main() { let local = 5; print(local); }
        "#,
    );
    assert!(
        !js.contains("__hmr_"),
        "a function-local must carry no HMR instrumentation: {js}"
    );
    assert!(
        js.contains("const local = 5;"),
        "the local emits its plain declaration: {js}"
    );
}

#[test]
fn hmr_module_and_local_same_name_only_wraps_the_module_binding() {
    // A module `mut n` and a same-named function-local `let n` must not collide:
    // only the module binding wraps + exposes under `pkg::n`; the local (which
    // would otherwise match the module seed on a swap and return the stale value)
    // emits unwrapped.
    let js = compile_hmr(
        r#"
        import std::io::print;
        mut n = 0;
        fun helper() { let n = 99; print(n); }
        fun main() { n = n + 1; helper(); print(n); }
        "#,
    );
    // The module binding wraps and is exposed.
    assert!(
        js.contains(r#"__hmr_adopt("pkg::n", "#),
        "the module binding wraps: {js}"
    );
    assert!(
        js.contains(r#"__hmr_expose("pkg::n", "#),
        "the module binding is exposed: {js}"
    );
    // Exactly one adopt and one expose — the local did not add its own.
    assert_eq!(
        js.matches("__hmr_adopt").count(),
        1,
        "only the module binding adopts, not the local: {js}"
    );
    assert_eq!(
        js.matches("__hmr_expose").count(),
        1,
        "only the module binding is exposed, not the local: {js}"
    );
    // The local `n` inside `helper` emits its own plain declaration.
    assert!(
        js.contains("= 99;"),
        "the same-named local emits unwrapped: {js}"
    );
}

#[test]
fn hmr_closure_body_local_is_not_wrapped() {
    // A local minted inside a closure body is likewise function-minted state, not
    // module-level — no adopt wrap.
    let js = compile_hmr(
        r#"
        import std::io::print;
        let make = || { let inner = 7; inner };
        fun main() { print(make()); }
        "#,
    );
    assert!(
        !js.contains(r#"__hmr_adopt("pkg::inner""#),
        "a closure-body local must not be wrapped: {js}"
    );
}

#[test]
fn hmr_disabled_is_byte_identical_and_has_no_instrumentation() {
    // The flag-off path is byte-identical to a normal browser compile, and carries
    // no `__hmr_` tokens at all — the equivalence-gate guarantee that `build`
    // output is untouched.
    let source = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        mut count = 0;
        let ticker = Signal::new(0);
        fun main() { count = count + 1; print(count); print(ticker.get()); }
        "#;
    let off = compile_browser_with_hmr(source, false).expect("compiles with hmr off");
    let baseline = compile_browser(source).expect("compiles normally");
    assert_eq!(
        off, baseline,
        "hmr = false must be byte-identical to a normal compile"
    );
    assert!(
        !off.contains("__hmr_"),
        "a non-HMR compile carries no HMR instrumentation: {off}"
    );
}

// --- A13 S2b: the dev::stash / dev::take transfer bound (hmr.md §4) -----------

/// A closure argument to `stash` is rejected — it carries code the new bundle
/// cannot adopt, the one thing the transfer bound forbids.
#[test]
fn hmr_stash_rejects_a_closure_argument() {
    assert_fails_browser_with(
        r#"
        import std::dev;

        fun main() {
            dev::stash("handler", || 0);
        }
        "#,
        "cannot cross a hot swap",
    );
}

/// A `Shared` argument to `stash` is rejected — a reactive cell's identity (its
/// subscribers) does not survive a swap; only a plain value would.
#[test]
fn hmr_stash_rejects_a_shared_argument() {
    assert_fails_browser_with(
        r#"
        import std::dev;
        import std::shared::Shared;

        fun main() {
            dev::stash("cell", Shared::new(0));
        }
        "#,
        "cannot cross a hot swap",
    );
}

/// The bound is by containment: a struct whose field holds a closure is rejected
/// just as a bare closure is.
#[test]
fn hmr_stash_rejects_a_struct_that_holds_a_closure() {
    assert_fails_browser_with(
        r#"
        import std::dev;

        struct Handlers { on_click: || void }

        fun main() {
            dev::stash("handlers", Handlers { on_click = || {} });
        }
        "#,
        "cannot cross a hot swap",
    );
}

/// A plain-data struct stashes cleanly — the bound admits scalars, `str`, lists,
/// options, and structs/enums built from them.
#[test]
fn hmr_stash_accepts_a_plain_struct() {
    assert_compiles_browser(
        r#"
        import std::dev;

        struct Session { id: i32, name: str }

        fun main() {
            dev::stash("session", Session { id = 1, name = "ada" });
        }
        "#,
    );
}

/// `take` is bound the same way: an annotated non-transferable element is
/// rejected at the call site.
#[test]
fn hmr_take_rejects_a_non_transferable_element() {
    assert_fails_browser_with(
        r#"
        import std::dev;
        import std::shared::Shared;
        import std::option::Option::{ self, Some, None };

        fun main() {
            let cell: Option<Shared<i32>> = dev::take("cell");
            match cell {
                Some(let c) => {},
                None => {},
            }
        }
        "#,
        "cannot cross a hot swap",
    );
}

/// A plain-data `take` and `on_teardown` compile cleanly — the hooks are inert
/// std surface without a shim, and their bounds admit plain data.
#[test]
fn hmr_take_and_on_teardown_accept_plain_usage() {
    assert_compiles_browser(
        r#"
        import std::dev;
        import std::option::Option::{ self, Some, None };

        fun main() {
            dev::on_teardown(|| {});
            let count: Option<i32> = dev::take("count");
            match count {
                Some(let n) => {},
                None => {},
            }
        }
        "#,
    );
}
