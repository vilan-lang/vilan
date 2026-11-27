//! Struct keys, slice S3a (B711, `proposal/struct-keys.md` §3.4, R2-R4):
//! `MaybeKey<S, F>`, a key to a place that may not be there — an `Option`'s
//! payload (`Doc.title::Some`) and a variant's payload slot
//! (`Doc.shape::Rect.0`) — reached by continuation (`get`, `view`, `change`,
//! `put`), with identity and composition, on the JS backend. The native
//! twins are `native_differential`'s `maybe_keys_*` programs.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

/// A struct with every kind of partial place: an optional scalar, an
/// optional struct, an enum with a one-slot, a two-slot and a unit variant.
const MAYBE_PRELUDE: &str = r#"
enum Shape { Circle(f64), Rect(f64, f64), Dot }
struct User { name: str, age: i32 }
struct Doc { title: Option<str>, owner: Option<User>, shape: Shape, n: i32 }

fun doc(): Doc {
	Doc { title = Some("a"), owner = Some(User { name = "ada", age = 3 }), shape = Shape::Rect(1.0, 2.0), n = 0 }
}

fun empty(): Doc {
	Doc { title = None, owner = None, shape = Shape::Circle(5.0), n = 0 }
}
"#;

fn maybe_runs(body: &str, expected_stdout: &str) {
    assert_compiles_and_runs(&format!("{MAYBE_PRELUDE}\n{body}"), expected_stdout);
}

fn maybe_refused(body: &str, message_part: &str) {
    assert_fails_with(&format!("{MAYBE_PRELUDE}\n{body}"), message_part);
}

// --- the literal ------------------------------------------------------------

/// Each literal form is a `MaybeKey` of the place's type — the annotations
/// are the assertion: an `Option`'s payload, a one-slot payload with and
/// without its position, one slot of a two-slot payload, and steps past a
/// partial step (a field of the payload struct).
#[test]
fn maybe_keys_every_literal_form_is_a_maybe_key_of_the_place_type() {
    maybe_runs(
        r#"
        fun main() {
            let title: MaybeKey<Doc, str> = Doc.title::Some;
            let owner: MaybeKey<Doc, User> = Doc.owner::Some;
            let name: MaybeKey<Doc, str> = Doc.owner::Some.name;
            let radius: MaybeKey<Doc, f64> = Doc.shape::Circle;
            let radius_at: MaybeKey<Doc, f64> = Doc.shape::Circle.0;
            let width: MaybeKey<Doc, f64> = Doc.shape::Rect.0;
            let height: MaybeKey<Doc, f64> = Doc.shape::Rect.1;
            let d = doc();
            print(title.get(&d));
            print(owner.get(&d).map(|u| u.age));
            print(name.get(&d));
            print(radius.get(&d));
            print(radius == radius_at);
            print(width.get(&d));
            print(height.get(&d));
        }
        "#,
        "Some(\"a\")\nSome(3)\nSome(\"ada\")\nNone\ntrue\nSome(1.0)\nSome(2.0)\n",
    );
}

/// Variant steps continue past a variant step: `Option<Option<T>>` steps
/// twice, a payload struct's field, and a variant step after a field after a
/// variant step.
#[test]
fn maybe_keys_steps_continue_past_a_partial_step() {
    assert_compiles_and_runs(
        r#"
        enum Slot { Empty, Held(Inner) }
        struct Inner { label: Option<str>, depth: Option<Option<i32>> }
        struct Root { slot: Slot }

        fun main() {
            mut root = Root { slot = Slot::Held(Inner { label = Some("x"), depth = Some(Some(4)) }) };
            let label: MaybeKey<Root, str> = Root.slot::Held.label::Some;
            let depth: MaybeKey<Root, i32> = Root.slot::Held.depth::Some::Some;
            print(label.get(&root));
            print(depth.get(&root));
            print(depth.put(&mut root, 9));
            print(depth.get(&root));
            print(label.name());
            print(depth.name());
            root.slot = Slot::Empty;
            print(label.get(&root));
            print(depth.put(&mut root, 1));
        }
        "#,
        "Some(\"x\")\nSome(4)\ntrue\nSome(9)\nslot::Held.label::Some\nslot::Held.depth::Some::Some\nNone\nfalse\n",
    );
}

/// A variant step reaches a generic enum's payload at the arguments the
/// field gives it, and a generic ROOT takes its arguments from the
/// expectation (R8) through a variant step too.
#[test]
fn maybe_keys_generic_enums_and_generic_roots() {
    assert_compiles_and_runs(
        r#"
        struct Pair<T> { left: Option<T>, right: Result<T, str> }
        struct Holder { pair: Pair<i32> }

        fun main() {
            mut h = Holder { pair = Pair { left = Some(1), right = Err("no") } };
            let left: MaybeKey<Holder, i32> = Holder.pair.left::Some;
            let ok: MaybeKey<Holder, i32> = Holder.pair.right::Ok;
            let err: MaybeKey<Holder, str> = Holder.pair.right::Err;
            print(left.get(&h));
            print(ok.get(&h));
            print(err.get(&h));
            let direct: MaybeKey<Pair<i32>, i32> = Pair.left::Some;
            print(direct.get(&h.pair));
            print(left.put(&mut h, 7));
            print(h.pair.left);
        }
        "#,
        "Some(1)\nNone\nSome(\"no\")\nSome(1)\ntrue\nSome(7)\n",
    );
}

// --- access by continuation (R3) --------------------------------------------

/// `get` copies the place out, and an aggregate copy shares nothing with the
/// subject.
#[test]
fn maybe_keys_get_copies_the_place_out() {
    maybe_runs(
        r#"
        fun main() {
            mut d = doc();
            let copy = Doc.owner::Some.get(&d);
            d.owner = Some(User { name = "bob", age = 1 });
            print(copy.map(|u| u.name));
            print(Doc.owner::Some.name.get(&d));
            print(Doc.title::Some.get(&empty()));
        }
        "#,
        "Some(\"ada\")\nSome(\"bob\")\nNone\n",
    );
}

/// `view` lends the place and answers whether it was there; `change` lends it
/// for writing, in place, at a scalar and at a struct.
#[test]
fn maybe_keys_view_and_change_lend_the_place() {
    maybe_runs(
        r#"
        fun main() {
            mut d = doc();
            print(Doc.title::Some.view(&d, |t| print(*t)));
            print(Doc.shape::Rect.1.change(&mut d, |h| { h = *h * 10.0; }));
            print(Doc.owner::Some.change(&mut d, |u| { u.age += 1; }));
            print(Doc.owner::Some.age.change(&mut d, |a| { a += 100; }));
            match d.shape { Shape::Rect(let w, let h) => print(i"{w} {h}"), _ => {} }
            print(d.owner.map(|u| u.age));
            mut e = empty();
            print(Doc.title::Some.view(&e, |_| print("never")));
            print(Doc.owner::Some.change(&mut e, |u| { u.age = 1; }));
            print(Doc.shape::Rect.0.change(&mut e, |w| { w = 0.0; }));
            print(e.owner is None);
        }
        "#,
        "a\ntrue\ntrue\ntrue\ntrue\n1 20\nSome(104)\nfalse\nfalse\nfalse\ntrue\n",
    );
}

/// `put` writes a place that is there and never creates one: `None` stays
/// `None`, another variant stays as it was.
#[test]
fn maybe_keys_put_never_creates_a_missing_place() {
    maybe_runs(
        r#"
        fun main() {
            mut d = doc();
            print(Doc.title::Some.put(&mut d, "b"));
            print(d.title);
            mut e = empty();
            print(Doc.title::Some.put(&mut e, "c"));
            print(e.title);
            print(Doc.shape::Rect.0.put(&mut e, 3.0));
            match e.shape { Shape::Circle(let r) => print(r), _ => print("moved") }
        }
        "#,
        "true\nSome(\"b\")\nfalse\nNone\nfalse\n5\n",
    );
}

/// A key held in a value — a binding, a list element, a parameter of a
/// generic function — reaches its place through the run-time walk.
#[test]
fn maybe_keys_held_in_values_and_generic_functions() {
    maybe_runs(
        r#"
        fun bump<S>(subject: &mut S, key: MaybeKey<S, f64>): bool {
            key.change(subject, |x| { x = *x + 1.0; })
        }

        fun read<S, F>(subject: &S, key: MaybeKey<S, F>): Option<F> {
            key.get(subject)
        }

        fun main() {
            mut d = doc();
            let keys: List<MaybeKey<Doc, f64>> = [Doc.shape::Rect.0, Doc.shape::Rect.1, Doc.shape::Circle];
            for key in keys {
                print(bump(&mut d, key));
            }
            match d.shape { Shape::Rect(let w, let h) => print(i"{w} {h}"), _ => {} }
            print(read(&d, Doc.owner::Some.name));
            print(read(&d, Doc.title::Some));
        }
        "#,
        "true\ntrue\nfalse\n2 3\nSome(\"ada\")\nSome(\"a\")\n",
    );
}

/// A view parameter is a subject like a binding: the continuation writes
/// through it.
#[test]
fn maybe_keys_reach_through_a_view_subject() {
    maybe_runs(
        r#"
        fun rename(d: &mut Doc, to: str): bool {
            Doc.owner::Some.name.put(d, to)
        }

        fun main() {
            mut d = doc();
            print(rename(&mut d, "cy"));
            print(d.owner.map(|u| u.name));
        }
        "#,
        "true\nSome(\"cy\")\n",
    );
}

// --- composition -------------------------------------------------------------

/// `then` across the four combinations, `maybe()`, and `some()` on a total
/// and on a partial key whose field is an `Option`; a total literal takes
/// the `MaybeKey` it lands in.
#[test]
fn maybe_keys_compose_total_and_partial() {
    assert_compiles_and_runs(
        r#"
        struct Inner { v: Option<i32>, w: i32 }
        struct Mid { inner: Option<Inner>, plain: Inner }
        struct Top { mid: Mid }

        fun main() {
            let t = Top { mid = Mid { inner = Some(Inner { v = Some(1), w = 2 }), plain = Inner { v = None, w = 3 } } };
            // total then total is total
            let total: Key<Top, i32> = Top.mid.then(Mid.plain).then(Inner.w);
            print(t[total]);
            // total then partial, through `maybe()`
            let a: MaybeKey<Top, Inner> = Top.mid.maybe().then(Mid.inner::Some);
            // partial then total: a total literal is a `MaybeKey` where one is expected
            let b: MaybeKey<Top, i32> = a.then(Inner.w);
            // partial then partial
            let c: MaybeKey<Top, i32> = a.then(Inner.v::Some);
            print(b.get(&t));
            print(c.get(&t));
            print(b == Top.mid.inner::Some.w);
            print(c == Top.mid.inner::Some.v::Some);
            // `some()` on a total and on a partial key to an `Option`
            print(Top.mid.inner.some() == Top.mid.inner::Some);
            print(a.then(Inner.v).some() == c);
            print(Top.mid.inner.some().then(Inner.v).some().get(&t));
            let widened: MaybeKey<Top, i32> = total.maybe();
            print(widened.get(&t));
            print(widened.path() == total.path());
        }
        "#,
        "3\nSome(2)\nSome(1)\ntrue\ntrue\ntrue\ntrue\nSome(1)\nSome(3)\ntrue\n",
    );
}

/// `some()` in generic code: a key to any `Option<T>` field.
#[test]
fn maybe_keys_some_in_generic_code() {
    maybe_runs(
        r#"
        fun payload<S, T>(key: Key<S, Option<T>>, subject: &S): Option<T> {
            key.some().get(subject)
        }

        fun main() {
            let d = doc();
            print(payload(Doc.title, &d));
            print(payload(Doc.owner, &d).map(|u| u.name));
            print(payload(Doc.title, &empty()));
        }
        "#,
        "Some(\"a\")\nSome(\"ada\")\nNone\n",
    );
}

// --- identity and the laws ---------------------------------------------------

/// Equality and the hash are the path's, however the key was written (a
/// table keyed by keys has one entry per place); the name spells each step as
/// a literal writes it after its root.
#[test]
fn maybe_keys_identity_and_names() {
    maybe_runs(
        r#"
        import std::hash_map::HashMap;

        fun main() {
            mut labels: HashMap<MaybeKey<Doc, str>, str> = HashMap::new();
            labels.insert(Doc.owner::Some.name, "owner's name");
            labels.insert(Doc.title::Some, "title");
            print(labels.get(Doc.owner.some().then(User.name)).unwrap_or("none"));
            print(labels.get(Doc.title.some()).unwrap_or("none"));
            print(labels.len());
            print(Doc.shape::Rect.0 == Doc.shape::Rect.1);
            print(Doc.shape::Rect.0 == Doc.shape::Rect.0);
            print(Doc.shape::Rect.0.name());
            print(Doc.shape::Circle.name());
            print(Doc.owner::Some.name.name());
            print(Doc.owner::Some.name.path().name());
        }
        "#,
        "owner's name\ntitle\n2\nfalse\ntrue\nshape::Rect.0\nshape::Circle\nowner::Some.name\nowner::Some.name\n",
    );
}

/// The laws for partial keys (§3.3): K3 (equal paths reach the same place),
/// K4 (paths where neither starts with the other are disjoint: a write
/// through one changes nothing the other reads) and K5 (`then` composes the
/// paths).
#[test]
fn maybe_keys_obey_k3_k4_k5() {
    maybe_runs(
        r#"
        fun main() {
            mut d = doc();
            // K3: two spellings, one place.
            let a = Doc.owner::Some.age;
            let b: MaybeKey<Doc, i32> = Doc.owner.some().then(User.age);
            print(a == b);
            print(a.put(&mut d, 40));
            print(b.get(&d));
            // K4: the two slots of one variant are disjoint, and so are two
            // variants; neither path starts with the other.
            let width = Doc.shape::Rect.0;
            let height = Doc.shape::Rect.1;
            let radius = Doc.shape::Circle;
            print(width.starts_with(height) || height.starts_with(width));
            print(width.starts_with(radius) || radius.starts_with(width));
            print(width.put(&mut d, 8.0));
            print(height.get(&d));
            print(radius.get(&d));
            // A variant step starts with the field it steps from.
            print(width.starts_with(Doc.shape));
            print(Doc.shape.path().starts_with(width.path()));
            // K5: `a.then(b).path() == a.path().then(b.path())`.
            let first = Doc.owner::Some;
            let second: MaybeKey<User, str> = User.name;
            print(first.then(second).path() == first.path().then(second.path()));
        }
        "#,
        "true\ntrue\nSome(40)\nfalse\nfalse\ntrue\nSome(2.0)\nNone\ntrue\nfalse\ntrue\n",
    );
}

/// A step tells its kind: a variant step has a payload position and may
/// miss, a field step neither; `Debug` writes a key as it is written.
#[test]
fn maybe_keys_steps_tell_their_kind_and_debug_as_written() {
    maybe_runs(
        r#"
        import std::key::KeyStep;

        fun show(step: KeyStep) {
            print(i"{step.name()} {step.index()} {step.position().unwrap_or(9)} {step.may_miss()}");
        }

        fun main() {
            for step in Doc.shape::Rect.1.path().steps() {
                show(step);
            }
            for step in Doc.owner::Some.name.path().steps() {
                show(step);
            }
            print(Doc.shape::Rect.1.debug());
            print(Doc.owner::Some.name.debug());
            print(Doc.owner::Some.name.path().debug());
        }
        "#,
        "shape 2 9 false\nRect 1 1 true\nowner 1 9 false\nSome 0 0 true\nname 0 9 false\nDoc.shape::Rect.1\nDoc.owner::Some.name\nKeyPath(Doc.owner::Some.name)\n",
    );
}

// --- refusals -----------------------------------------------------------------

#[test]
fn maybe_keys_an_unknown_variant_is_refused_with_a_suggestion() {
    maybe_refused(
        "fun main() { let k = Doc.shape::Rec.0; }",
        "enum 'Shape' has no variant 'Rec'; did you mean `Rect`?",
    );
}

#[test]
fn maybe_keys_a_unit_variant_has_no_place() {
    maybe_refused(
        "fun main() { let k = Doc.shape::Dot; }",
        "`Shape::Dot` is a unit variant: it holds no value, so there is no place for a key to reach; to test for it, read `Doc.shape` and write `is Shape::Dot`",
    );
}

#[test]
fn maybe_keys_a_position_is_required_for_a_multi_slot_payload_and_checked() {
    maybe_refused(
        "fun main() { let k = Doc.shape::Rect; }",
        "`Shape::Rect` holds 2 values, and a key reaches one slot at a time: name it by its position, `Doc.shape::Rect.0` to `Doc.shape::Rect.1`",
    );
    maybe_refused(
        "fun main() { let k = Doc.shape::Rect.2; }",
        "`Shape::Rect`'s payload has 2 slots, at positions 0 to 1: `.2` is out of range",
    );
    maybe_refused(
        "fun main() { let k = Doc.shape::Circle.1; }",
        "`Shape::Circle`'s payload has one slot, at position 0: `.1` is out of range",
    );
}

#[test]
fn maybe_keys_a_variant_step_needs_an_enum() {
    maybe_refused(
        "fun main() { let k = Doc.n::Some; }",
        "a variant step reaches into an `Option` or an enum's payload: `Doc.n` has type `i32`, which has no variants",
    );
}

/// A FIELD step into an enum steers to the variant step that reaches it.
#[test]
fn maybe_keys_a_field_step_into_an_option_steers_to_the_variant_step() {
    maybe_refused(
        "fun main() { let k = Doc.owner.name; }",
        "a field step reaches only struct fields: `Doc.owner` has type `Option<User>`, an enum: a key reaches a variant's payload with a variant step, `Doc.owner::Some.name`",
    );
    maybe_refused(
        "fun main() { let k = Doc.shape.x; }",
        "a field step reaches only struct fields: `Doc.shape` has type `Shape`, an enum: a key reaches a variant's payload with a variant step, `Doc.shape::Circle`",
    );
}

/// A variant step on a VALUE is no key: it is refused with the forms that
/// test a value's variant (R9: a value of the name is the value).
#[test]
fn maybe_keys_a_variant_step_on_a_value_is_refused() {
    maybe_refused(
        "fun main() { let d = doc(); let k = d.title::Some; }",
        "a variant step (`::Some`) stands only in a key literal's path, rooted at a struct type (`Doc.title::Some`), and `d.title` is a value: to test a value's variant write `d.title is Some(..)` or `match`",
    );
}

/// `s[k]` names no place a `MaybeKey` reaches (R3), to read or to write.
#[test]
fn maybe_keys_have_no_subscript() {
    let steer = "this key is a `MaybeKey`: its place may not be there, so `s[k]` cannot name it; read it with `k.get(&s)` (an `Option`), lend it with `k.view(&s, |place| ..)` or `k.change(&mut s, |place| ..)`, or write it with `k.put(&mut s, value)` (each answers whether it was there)";
    maybe_refused(
        "fun main() { let d = doc(); print(d[Doc.title::Some]); }",
        steer,
    );
    maybe_refused(
        "fun main() { mut d = doc(); d[Doc.title::Some] = \"x\"; }",
        steer,
    );
    maybe_refused(
        "fun main() { mut d = doc(); let k = Doc.shape::Rect.0; d[k] += 1.0; }",
        steer,
    );
}

/// A `MaybeKey` is no `Key`, and a total key VALUE becomes one by `.maybe()`.
#[test]
fn maybe_keys_and_keys_do_not_convert_silently() {
    maybe_refused(
        "fun total(k: Key<Doc, f64>) {} fun main() { total(Doc.shape::Circle); }",
        "Expected Key<Doc, f64>, but got MaybeKey<Doc, f64> instead: this key's path has a step that may find nothing (a variant step), so it is a `MaybeKey`, which reaches its place with `get`, `view`, `change` or `put`; a `Key` names a place that is always there",
    );
    maybe_refused(
        "fun partial(k: MaybeKey<Doc, i32>) {} fun main() { let k = Doc.n; partial(k); }",
        "Expected MaybeKey<Doc, i32>, but got Key<Doc, i32> instead: a total key held in a value becomes a `MaybeKey` with `.maybe()` (a key literal needs no call)",
    );
    // A literal needs no call.
    maybe_runs(
        "fun partial(k: MaybeKey<Doc, i32>): str { k.name() } fun main() { print(partial(Doc.n)); }",
        "n\n",
    );
}

/// A `MaybeKey` has no public constructor and nothing outside std opens it.
#[test]
fn maybe_keys_cannot_be_forged_or_opened() {
    maybe_refused(
        "fun main() { let k = Doc.title::Some; print(k.erased); }",
        "`erased` is an `[internal]` field of std's `MaybeKey`",
    );
    maybe_refused(
        "import std::key::KeyPath; fun forge(p: KeyPath): MaybeKey<Doc, str> { MaybeKey { erased = p } } fun main() {}",
        "`erased` is an `[internal]` field of std's `MaybeKey`",
    );
}

/// The half-typed `Doc.title::` is refused with the variants it could name.
#[test]
fn maybe_keys_a_half_typed_variant_step_lists_the_variants() {
    maybe_refused(
        "fun main() { let k = Doc.shape::; }",
        "expected a variant name after `::`: `Doc.shape` is a `Shape`, whose payload variants are `::Circle`, `::Rect`",
    );
}

/// `::` after a postfix that is no member is still the error it was.
#[test]
fn maybe_keys_a_path_after_a_call_still_fails_to_parse() {
    assert_fails("fun f(): i32 { 1 } fun main() { let x = f()::Some; }");
}

// --- emission -----------------------------------------------------------------

/// A partial literal is one hoisted constant whose variant step carries its
/// kind (`[1, position, width]`); a total literal's steps carry `[0]`. A
/// continuation through a LITERAL calls one accessor function per literal
/// and access — the walk the literal spells, a tag test per variant step,
/// as a hand-written `match` is — and through a key HELD in a value it walks
/// the key's steps (`__mkey_get`).
#[test]
fn maybe_keys_a_literal_is_one_constant_and_one_accessor_per_access() {
    let js = compile(&format!(
        "{MAYBE_PRELUDE}\nfun held(k: MaybeKey<Doc, f64>, d: Doc): Option<f64> {{ k.get(&d) }}\n\
         fun main() {{ let d = doc(); print(Doc.shape::Rect.1.get(&d)); print(Doc.shape::Rect.1.get(&d)); \
         print(held(Doc.shape::Rect.1, d)); }}"
    ))
    .expect("compiles");
    assert_eq!(
        js.matches("\"Rect\", [ 1, 1, 2 ] ]").count(),
        1,
        "one constant for the literal, with its variant step's kind:\n{js}"
    );
    assert!(
        js.contains("\"shape\", [ 0 ] ]"),
        "a field step's kind is `[0]`:\n{js}"
    );
    assert_eq!(
        js.matches("if (v1[0] !== 1) {").count(),
        1,
        "one accessor for the literal's `get`, testing `Rect`'s tag:\n{js}"
    );
    assert!(
        js.contains("return [ 0, v1[2] ];"),
        "the slot after the tag:\n{js}"
    );
    assert!(
        js.contains("__mkey_get("),
        "a held key walks its steps:\n{js}"
    );
}

/// The compiler builds `std::key`'s values itself (both emitters and the
/// const evaluator write the layout and walk it by position), so the field
/// ORDER of `Key`, `MaybeKey`, `KeyPath` and `KeyStep`, and the variant order
/// of `StepKind`, is the representation: this pins it against the source.
#[test]
fn keys_std_layout() {
    let source = include_str!("../../../../vilan/std/src/key.vl");
    let declared = |head: &str| -> Vec<String> {
        let start = source
            .find(head)
            .unwrap_or_else(|| panic!("`{head}` in std::key"));
        let body = &source[start + head.len()..];
        let body = &body[..body.find("\n}").expect("the declaration closes")];
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("///"))
            .map(|line| {
                // `[internal("..")] name: T,` or `Variant(..),`
                let line = match line.rfind(")] ") {
                    Some(at) if line.starts_with('[') => &line[at + 3..],
                    _ => line,
                };
                line.split([':', '(', ','])
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .collect()
    };
    assert_eq!(declared("export struct Key<S, F> {"), ["erased"]);
    assert_eq!(declared("export struct MaybeKey<S, F> {"), ["erased"]);
    assert_eq!(declared("export struct KeyPath {"), ["parts"]);
    assert_eq!(
        declared("export struct KeyStep {"),
        ["owner", "index", "name", "kind"]
    );
    assert_eq!(declared("enum StepKind {"), ["Field", "Variant"]);
}

/// The continuations call their closure before they return and keep
/// nothing, so a closure capturing a VIEW may be handed to one (an external
/// with no readable body is otherwise assumed to keep what it is handed).
#[test]
fn maybe_keys_a_continuation_takes_a_closure_capturing_a_view() {
    maybe_runs(
        r#"
        fun total(d: &Doc, acc: &mut i32): bool {
            Doc.owner::Some.age.view(d, |age| {
                acc = *acc + *age;
            })
        }

        fun main() {
            let d = doc();
            mut sum = 10;
            print(total(&d, &mut sum));
            print(sum);
        }
        "#,
        "true\n13\n",
    );
}

/// A partial key folds in a `const` expression, and so does a function that
/// reaches places through one (the const evaluator walks the key as the
/// emitted helpers do).
#[test]
fn maybe_keys_fold_in_a_const_expression() {
    maybe_runs(
        r#"
        fun grown(): Doc {
            mut d = doc();
            Doc.owner::Some.age.change(&mut d, |a| { a += 4; });
            Doc.shape::Rect.1.put(&mut d, 9.0);
            Doc.shape::Circle.put(&mut d, 1.0);
            d
        }

        fun main() {
            let key = const Doc.owner::Some.name;
            print(key.get(&doc()));
            print(const Doc.shape::Rect.0.name());
            let d = const grown();
            print(d.owner.map(|u| u.age));
            match d.shape { Shape::Rect(let w, let h) => print(i"{w} {h}"), _ => {} }
            print(const Doc.title::Some.get(&empty()));
        }
        "#,
        "Some(\"ada\")\nshape::Rect.0\nSome(7)\n1 9\nNone\n",
    );
}
