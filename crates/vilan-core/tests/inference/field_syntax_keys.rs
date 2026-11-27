//! Struct keys, slice S2a (B711, `proposal/struct-keys.md` §3.5, R6): the
//! handle door. On a receiver whose type has a `[field_syntax]` method taking
//! a `Key<T, _>`, a member that is no member of the receiver and names a field
//! of `T` reads as `receiver.method(T.member)`, checked, typed and emitted as
//! that call. The store rule (A149 S4, keyed on `Store`/`StoreSome`) stands
//! beside it unchanged; its pins are in `store.rs` and the editor's.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

/// A read/write cursor at a place inside a shared root, which composes keys
/// with `then`; and a root handle, its own type because S1 has no identity
/// key (`Key<S, S>`) to start a cursor at the root with.
const HANDLES: &str = r#"
import std::shared::Shared;

struct Address { city: str, zip: i32 }
struct User { name: str, address: Address }

struct Cursor<S, T> {
	root: Shared<S>,
	key: Key<S, T>,
}

impl Cursor<type S, type T> {
	fun get(self): T {
		self.root.read()[self.key]
	}

	fun set(self, value: T) {
		self.root.write()[self.key] = value;
	}

	[field_syntax]
	fun at<U>(self, key: Key<T, U>): Cursor<S, U> {
		Cursor { root = self.root, key = self.key.then(key) }
	}
}

struct Root<S> {
	state: Shared<S>,
}

impl Root<type S> {
	[field_syntax]
	fun at<U>(self, key: Key<S, U>): Cursor<S, U> {
		Cursor { root = self.state, key }
	}
}

fun user(): Root<User> {
	Root { state = Shared::new(User { name = "ada", address = Address { city = "london", zip = 1 } }) }
}
"#;

fn handles_run(body: &str, expected_stdout: &str) {
    assert_compiles_and_runs(&format!("{HANDLES}\n{body}"), expected_stdout);
}

fn handles_refused(body: &str, message_part: &str) {
    assert_fails_with(&format!("{HANDLES}\n{body}"), message_part);
}

// --- what reads through ------------------------------------------------------

/// One step: `r.name` is `r.at(User.name)`, typed as its result.
#[test]
fn field_syntax_one_step_reads_as_the_method_call() {
    handles_run(
        r#"
        fun main() {
            let r = user();
            let name: Cursor<User, str> = r.name;
            print(name.get());
            print(r.at(User.name).get());
        }
        "#,
        "ada\nada\n",
    );
}

/// A chain follows from each read's result type, and a write through the
/// chain's last cursor reaches the root.
#[test]
fn field_syntax_a_chain_reads_through_each_result() {
    handles_run(
        r#"
        fun main() {
            let r = user();
            let city = r.address.city;
            city.set("paris");
            print(r.address.city.get());
            print(r.state.read().address.city);
            r.address.zip.set(r.address.zip.get() + 41);
            print(r.state.read().address.zip);
        }
        "#,
        "paris\nparis\n42\n",
    );
}

/// A generic struct `T` at concrete arguments: the key takes its arguments
/// from the method's parameter (R8), and the result is at the field's type.
#[test]
fn field_syntax_a_generic_struct_reads_at_its_arguments() {
    assert_compiles_and_runs(
        r#"
        import std::shared::Shared;
        struct Pair<T> { left: T, right: T }
        struct Wrap { pair: Pair<str>, n: i32 }
        struct H<T> { root: Shared<T> }
        impl H<type T> {
            [field_syntax]
            fun at<U>(self, key: Key<T, U>): H<U> { H { root = Shared::new(self.root.read()[key]) } }
            fun value(self): T { self.root.read() }
        }
        fun main() {
            let p = H { root = Shared::new(Pair { left = 1, right = 2 }) };
            let left: H<i32> = p.left;
            print(left.value() + p.right.value());
            let w = H { root = Shared::new(Wrap { pair = Pair { left = "l", right = "r" }, n = 3 }) };
            let right: H<str> = w.pair.right;
            print(right.value());
        }
        "#,
        "3\nr\n",
    );
}

/// The handle's own FIELD wins: `c.key` is the cursor's key, not `T`'s field
/// of that name.
#[test]
fn field_syntax_the_handles_own_field_wins() {
    assert_compiles_and_runs(
        r#"
        import std::shared::Shared;
        struct Lock { key: str, state: str }
        struct H<T> { root: Shared<T>, key: i32 }
        impl H<type T> {
            [field_syntax]
            fun at<U>(self, key: Key<T, U>): U { self.root.read()[key] }
        }
        fun main() {
            let h = H { root = Shared::new(Lock { key = "k", state = "s" }), key = 7 };
            print(h.key);
            print(h.state);
            print(h.at(Lock.key));
        }
        "#,
        "7\ns\nk\n",
    );
}

/// A field named like one of the handle's METHODS is shadowed; the written
/// call reaches it, and the refusal for the bare read names both forms.
#[test]
fn field_syntax_a_method_named_field_is_reached_by_the_written_call() {
    let program = r#"
        import std::shared::Shared;
        struct Request { get: str, body: str }
        struct H<T> { root: Shared<T> }
        impl H<type T> {
            fun get(self): T { self.root.read() }
            [field_syntax]
            fun at<U>(self, key: Key<T, U>): U { self.root.read()[key] }
        }
        fun main() {
            let h = H { root = Shared::new(Request { get = "verb", body = "b" }) };
            print(h.at(Request.get));
            print(h.body);
            print(h.get().get);
        }
        "#;
    assert_compiles_and_runs(program, "verb\nb\nverb\n");
    assert_fails_with(
        &program.replace("print(h.body);", "print(h.get);"),
        "`get` is a method of `H`: call it, `h.get()`; `get` is also a field of `Request`, which \
         the method shadows from field syntax: reach the field with `h.at(Request.get)`",
    );
}

/// A receiver behind a view reads through as a value does.
#[test]
fn field_syntax_a_receiver_behind_a_view_reads_through() {
    handles_run(
        r#"
        fun city_of(r: &Root<User>): str {
            r.address.city.get()
        }
        fun main() {
            let r = user();
            print(city_of(&r));
        }
        "#,
        "london\n",
    );
}

/// The handle's `T` is a type parameter at this receiver: the rule says
/// nothing, and the ordinary refusal stands.
#[test]
fn field_syntax_a_type_parameter_subject_is_inapplicable() {
    handles_refused(
        r#"
        fun read<S>(r: Root<S>) {
            let x = r.name;
        }
        fun main() {
            read(user());
        }
        "#,
        "struct 'Root' has no field 'name'",
    );
}

/// A receiver that is ITSELF a type parameter: no field syntax, the plain
/// refusal.
#[test]
fn field_syntax_a_type_parameter_receiver_is_inapplicable() {
    handles_refused(
        r#"
        fun read<R>(r: R) {
            let x = r.name;
        }
        fun main() {
            read(user());
        }
        "#,
        "cannot access field 'name' on type R",
    );
}

/// A member that names no field of `T`: the plain refusal on the handle.
#[test]
fn field_syntax_a_member_naming_no_field_is_the_plain_refusal() {
    handles_refused(
        "fun main() { let x = user().nmae; }",
        "struct 'Root' has no field 'nmae'",
    );
}

// --- the result is a value -------------------------------------------------

#[test]
fn field_syntax_a_read_is_no_place() {
    handles_refused(
        r#"fun main() { let r = user(); r.name = "x"; }"#,
        "`r.name` reads through `[field_syntax]` as `r.at(User.name)`, a call, which answers a \
         `Cursor<User, str>`: a value, not a place - write through it with `.set(..)`",
    );
}

// --- what a key literal refuses, field syntax refuses -----------------------

#[test]
fn field_syntax_an_internal_std_field_is_refused_as_its_key() {
    assert_fails_with(
        r#"
        import std::shared::Shared;
        struct Task { title: str }
        struct H<T> { root: Shared<T> }
        impl H<type T> {
            [field_syntax]
            fun at<U>(self, key: Key<T, U>): i32 { 1 }
        }
        fun main() {
            let h = H { root = Shared::new(Task.title.path()) };
            let n = h.parts;
        }
        "#,
        "`parts` reads through `[field_syntax]` as `h.at(KeyPath.parts)`, and that key is refused: \
         `parts` is an `[internal]` field of std's `KeyPath`, and an internal std field is not a \
         member outside std, so it has no key",
    );
}

#[test]
fn field_syntax_a_resources_field_is_refused_as_its_key() {
    assert_fails_with(
        r#"
        [resource]
        struct Fd { n: i32 }
        struct R {}
        impl R {
            [field_syntax]
            fun at<U>(self, key: Key<Fd, U>): i32 { 1 }
        }
        fun main() {
            let r = R {};
            let n = r.n;
        }
        "#,
        "`n` reads through `[field_syntax]` as `r.at(Fd.n)`, and that key is refused: `Fd` is a \
         resource, and a key cannot reach into a resource's fields",
    );
}

// --- the declaration ---------------------------------------------------------

const SHAPE: &str = "a `[field_syntax]` method has the shape `fun at<U>(self, key: Key<T, U>): R`";

fn declaration_refused(method: &str, message_part: &str) {
    let source = format!(
        "struct A {{ x: i32 }}\nstruct B {{}}\nimpl B {{\n\t{method}\n}}\nfun main() {{}}\n"
    );
    assert_fails_with(&source, SHAPE);
    assert_fails_with(&source, message_part);
}

#[test]
fn field_syntax_the_declarations_shape_is_checked() {
    for (method, message_part) in [
        (
            "[field_syntax] fun at<U>(&mut self, key: Key<A, U>): i32 { 1 }",
            "this one takes `&mut self`",
        ),
        (
            "[field_syntax] fun at<U>(own self, key: Key<A, U>): i32 { 1 }",
            "this one takes `own self`",
        ),
        (
            "[field_syntax] fun at<U>(self): i32 { 1 }",
            "this one takes no parameter after `self`",
        ),
        (
            "[field_syntax] fun at<U>(self, key: Key<A, U>, extra: i32): i32 { 1 }",
            "this one takes 2 parameters after `self`",
        ),
        (
            "[field_syntax] fun at<U>(self, key: i32): i32 { 1 }",
            "this one's parameter is no `Key<T, U>`",
        ),
        (
            "[field_syntax] fun at<U>(self, key: Key<A, i32>): i32 { 1 }",
            "this one's key does not end at the method's own type parameter",
        ),
        (
            "[field_syntax] fun at<U, V>(self, key: Key<A, U>): i32 { 1 }",
            "this one declares type parameters besides the field type",
        ),
        (
            "[field_syntax] fun at<U>(self, lazy key: Key<A, U>): i32 { 1 }",
            "this one's key parameter is not a plain value",
        ),
        (
            "[field_syntax] fun at<U>(self, key: &Key<A, U>): i32 { 1 }",
            "this one's key parameter is not a plain value",
        ),
        (
            "[field_syntax] fun at<U>(self, key: Key<(i32, i32), U>): i32 { 1 }",
            "this one's key is over neither a struct nor a type parameter of the impl",
        ),
        (
            "[field_syntax] fun at<U>(key: Key<A, U>): i32 { 1 }",
            "this one takes no `self`",
        ),
    ] {
        declaration_refused(method, message_part);
    }
}

/// `&self` is the other admitted receiver.
#[test]
fn field_syntax_a_ref_self_method_reads_through() {
    assert_compiles_and_runs(
        r#"
        struct A { x: i32 }
        struct B { a: A }
        impl B {
            [field_syntax]
            fun at<U>(&self, key: Key<A, U>): U { self.a[key] }
        }
        fun main() {
            let b = B { a = A { x = 5 } };
            print(b.x);
        }
        "#,
        "5\n",
    );
}

/// A refused declaration is one diagnostic: its reads fall back to the plain
/// refusal rather than to a second message per read.
#[test]
fn field_syntax_a_refused_declaration_is_not_read_through() {
    assert_fails_with(
        r#"
        struct A { x: i32 }
        struct B {}
        impl B {
            [field_syntax]
            fun at<U>(&mut self, key: Key<A, U>): i32 { 1 }
        }
        fun main() {
            let b = B {};
            let n = b.x;
        }
        "#,
        "struct 'B' has no field 'x'",
    );
}

#[test]
fn field_syntax_a_second_method_on_one_handle_is_refused() {
    assert_fails_with(
        r#"
        struct A { x: i32 }
        struct B {}
        impl B {
            [field_syntax]
            fun at<U>(self, key: Key<A, U>): i32 { 1 }
        }
        impl B {
            [field_syntax]
            fun to<U>(self, key: Key<A, U>): i32 { 2 }
        }
        fun main() {}
        "#,
        "`B` already has a `[field_syntax]` method, `at`: a handle type has one, so that \
         `handle.name` means one call - read through `to` by calling it",
    );
}

#[test]
fn field_syntax_a_traits_method_is_refused() {
    assert_fails_with(
        r#"
        struct A { x: i32 }
        trait Door {
            [field_syntax]
            fun at<U>(self, key: Key<A, U>): i32;
        }
        fun main() {}
        "#,
        "it may label a method of an inherent `impl`, not a trait's",
    );
}

#[test]
fn field_syntax_a_trait_impls_method_is_refused() {
    assert_fails_with(
        r#"
        struct A { x: i32 }
        trait Door {
            fun at<U>(self, key: Key<A, U>): i32;
        }
        struct B {}
        impl B with Door {
            [field_syntax]
            fun at<U>(self, key: Key<A, U>): i32 { 1 }
        }
        fun main() {}
        "#,
        "it may label a method of an inherent `impl`, not one that implements a trait",
    );
}

#[test]
fn field_syntax_the_attribute_off_a_method_is_refused() {
    assert_fails_with(
        "struct A { x: i32 }\n[field_syntax]\nfun free<U>(key: Key<A, U>): i32 { 1 }\nfun main() {}\n",
        "it may label only a method in an `impl`, and a free function has no receiver",
    );
    assert_fails_with(
        "[field_syntax]\nstruct A { x: i32 }\nfun main() {}\n",
        "`[field_syntax]` marks the method a handle's field syntax calls (`handle.name` is \
         `handle.at(T.name)`): it may label only a method in an `impl`",
    );
    assert_fails_with(
        "struct A { x: i32 }\nstruct B {}\nimpl B {\n\t[field_syntax]\n\texternal fun at<U>(self, key: Key<A, U>): i32;\n}\nfun main() {}\n",
        "it may label only a method with a body, and an `external fun` binds a host function",
    );
}

/// The impl must be on a struct: field syntax reads a member of a struct
/// value.
#[test]
fn field_syntax_an_impl_not_on_a_struct_is_refused() {
    assert_fails_with(
        r#"
        struct A { x: i32 }
        enum E { One }
        impl E {
            [field_syntax]
            fun at<U>(self, key: Key<A, U>): i32 { 1 }
        }
        fun main() {}
        "#,
        "`[field_syntax]` labels a method of a struct's inherent `impl`",
    );
}

/// B711 S3a: the door takes a `Key`, never a `MaybeKey` (S3c's), so a variant
/// step written through it (`r.name::Some`) is a variant step on a VALUE —
/// the read's result — and refused as one; a `MaybeKey` passed to the door
/// is refused as the `Key` it is not.
#[test]
fn field_syntax_a_variant_step_through_a_door_is_refused() {
    handles_refused(
        "fun main() { let r = user(); let k = r.address::Some; }",
        "a variant step (`::Some`) stands only in a key literal's path, rooted at a struct type (`Doc.title::Some`), and `r.address` is a value",
    );
    assert_fails_with(
        &format!(
            "{HANDLES}\nstruct Profile {{ nick: Option<str> }}\n\
             fun main() {{ let r = Root {{ state = Shared::new(Profile {{ nick = None }}) }}; let c = r.at(Profile.nick::Some); }}"
        ),
        "but got MaybeKey<Profile, str> instead: this key's path has a step that may find nothing",
    );
    // And a door declared over a `MaybeKey` is refused by the shape check
    // (the door for partial keys is S3c's).
    declaration_refused(
        "[field_syntax]\n\tfun at<U>(self, key: MaybeKey<A, U>): Option<U> { None }",
        "this one's parameter is no `Key<T, U>`",
    );
}
