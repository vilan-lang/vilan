//! Struct keys, slice S1a (B711, `proposal/struct-keys.md`): `S.x` and paths
//! as `Key<S, F>` values, `s[k]` as a place, `then`, identity (`PartialEq`,
//! `Hashable`, `KeyPath`) and the laws K1-K5 of §3.3, on the JS backend.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

/// The structs most pins read through: a three-level path (`Task.meta.owner`,
/// `Task.meta.stars`) and a sibling field of the same type at the root.
const KEYS_PRELUDE: &str = r#"
struct Meta { owner: str, stars: i32 }
struct Task { title: str, status: str, meta: Meta }

fun task(): Task {
	Task { title = "a", status = "open", meta = Meta { owner = "ada", stars = 1 } }
}
"#;

fn keys_runs(body: &str, expected_stdout: &str) {
    assert_compiles_and_runs(&format!("{KEYS_PRELUDE}\n{body}"), expected_stdout);
}

fn keys_refused(body: &str, message_part: &str) {
    assert_fails_with(&format!("{KEYS_PRELUDE}\n{body}"), message_part);
}

// --- the key expression -----------------------------------------------------

/// `S.x` is a `Key<S, F>`, and a path `S.a.b` is ONE key of the leaf's type —
/// the annotations are the assertion.
#[test]
fn keys_a_field_and_a_path_are_keys_of_the_field_type() {
    keys_runs(
        r#"
        fun main() {
            let title: Key<Task, str> = Task.title;
            let owner: Key<Task, str> = Task.meta.owner;
            let meta: Key<Task, Meta> = Task.meta;
            let stars: Key<Meta, i32> = Meta.stars;
            let t = task();
            print(t[title]);
            print(t[owner]);
            print(t[meta].stars);
            print(t[meta][stars]);
        }
        "#,
        "a\nada\n1\n1\n",
    );
}

#[test]
fn keys_a_key_of_the_wrong_field_type_is_refused() {
    keys_refused(
        "fun main() { let k: Key<Task, i32> = Task.title; }",
        "Expected Key<Task, i32>, but got Key<Task, str> instead",
    );
}

/// A three-step path, its steps through a struct holding a generic struct at a
/// concrete argument (`Pair<str>`).
#[test]
fn keys_a_three_step_path_reads_and_writes_the_leaf() {
    assert_compiles_and_runs(
        r#"
        struct C { z: i32 }
        struct B { c: C }
        struct A { b: B, n: i32 }
        struct Pair<T> { left: T, right: T }
        struct Boxed { pair: Pair<str> }

        fun main() {
            mut a = A { b = B { c = C { z = 1 } }, n = 5 };
            let deep: Key<A, i32> = A.b.c.z;
            a[deep] = 7;
            print(a.b.c.z);
            a[A.b.c.z] += 1;
            print(a[deep]);
            let bx = Boxed { pair = Pair { left = "q", right = "r" } };
            print(bx[Boxed.pair.right]);
        }
        "#,
        "7\n8\nr\n",
    );
}

// --- R9: a value named like a type is the value ------------------------------

/// Where `S` names both a type and a value in scope, the value wins: the
/// member access is the ordinary field read, as it was before keys.
#[test]
fn keys_a_value_named_like_the_type_is_the_value() {
    keys_runs(
        r#"
        fun read_param(Task: Task): str {
            Task.title
        }

        fun main() {
            let Task = task();
            print(Task.title);
            print(Task.meta.owner);
            print(read_param(task()));
        }
        "#,
        "a\nada\na\n",
    );
}

// --- access in place --------------------------------------------------------

/// `s[k]` is a place under the rules of `s.x`: read (a copy of the field
/// only), assignment, compound assignment, a write into a struct-typed field
/// (`s[k].inner = v`), and the same through a key held in a value.
#[test]
fn keys_a_subscript_is_a_place_like_a_field() {
    keys_runs(
        r#"
        fun main() {
            mut t = task();
            let title = Task.title;
            let meta = Task.meta;
            t[title] = "b";
            t[Task.status] = "closed";
            t[Task.meta.stars] += 2;
            t[meta].owner = "bob";
            t[Task.meta].stars *= 10;
            print(t.title);
            print(t.status);
            print(t.meta.stars);
            print(t.meta.owner);
            mut copied = t[meta];
            copied.owner = "copy";
            print(t.meta.owner);
            print(copied.owner);
        }
        "#,
        "b\nclosed\n30\nbob\nbob\ncopy\n",
    );
}

/// An immutable subject refuses a write through a key, as through a field.
#[test]
fn keys_a_write_through_a_key_needs_a_mutable_subject() {
    keys_refused(
        "fun main() { let t = task(); t[Task.title] = \"b\"; }",
        "cannot mutate immutable 't'",
    );
    keys_refused(
        "fun main() { let k = Task.title; let t = task(); t[k] = \"b\"; }",
        "cannot mutate immutable 't'",
    );
}

/// `&s[k]` and `&mut s[k]`, at a scalar field (the `(base, key)` pair) and at
/// a struct field, from a literal key and from one held in a value, over a
/// subject that is itself a view.
#[test]
fn keys_views_of_a_keyed_place() {
    keys_runs(
        r#"
        fun bump(n: &mut i32) {
            n += 1;
        }

        fun rename(m: &mut Meta) {
            m.owner = "cy";
        }

        fun peek(n: &i32): i32 {
            *n
        }

        fun through(t: &mut Task, k: Key<Task, i32>) {
            bump(&mut t[k]);
        }

        fun main() {
            mut t = task();
            bump(&mut t[Task.meta.stars]);
            let stars = Task.meta.stars;
            bump(&mut t[stars]);
            through(&mut t, stars);
            rename(&mut t[Task.meta]);
            print(t.meta.stars);
            print(t.meta.owner);
            print(peek(&t[stars]));
            let r = &mut t[stars];
            r = 10;
            print(t.meta.stars);
        }
        "#,
        "4\ncy\n4\n10\n",
    );
}

/// The accessor of §3.2, generic in the field type: a view return whose leaf
/// is `s[k]`, at a scalar `F` and at a struct `F`, written and read.
#[test]
fn keys_the_accessor_returns_a_view_at_scalar_and_struct_fields() {
    keys_runs(
        r#"
        fun at<F>(t: &mut Task, k: Key<Task, F>): &mut F borrows t {
            t[k]
        }

        fun main() {
            mut t = task();
            at(&mut t, Task.status) = "closed";
            at(&mut t, Task.meta.stars) += 4;
            print(t.status);
            print(t.meta.stars);
            print(*at(&mut t, Task.title));
            let m = at(&mut t, Task.meta);
            m.owner = "dee";
            print(t.meta.owner);
            let k = Task.meta.owner;
            at(&mut t, k) = "eve";
            print(t.meta.owner);
        }
        "#,
        "closed\n5\na\ndee\neve\n",
    );
}

/// Keys in generic functions: the field type is the function's parameter,
/// bound per call.
#[test]
fn keys_in_generic_functions() {
    keys_runs(
        r#"
        fun read<F>(t: Task, k: Key<Task, F>): F {
            t[k]
        }

        fun write<F>(t: &mut Task, k: Key<Task, F>, v: F) {
            t[k] = v;
        }

        fun read_any<S, F>(s: S, k: Key<S, F>): F {
            s[k]
        }

        fun main() {
            mut t = task();
            write(&mut t, Task.meta.owner, "bob");
            write(&mut t, Task.meta.stars, 9);
            print(read(t, Task.meta.owner));
            print(read(t, Task.meta.stars));
            print(read(t, Task.meta).owner);
            print(read_any(t, Task.title));
            print(read_any(t.meta, Meta.stars));
        }
        "#,
        "bob\n9\nbob\na\n9\n",
    );
}

/// A key's struct must be the subject's: a key over another struct, or a
/// subject that is no struct, is refused by name.
#[test]
fn keys_a_key_indexes_only_its_own_struct() {
    keys_refused(
        "struct Other { title: str } fun main() { let o = Other { title = \"x\" }; print(o[Task.title]); }",
        "this key reaches into a `Task`, and the subject is a `Other`: a key indexes only the struct it was made from",
    );
    keys_refused(
        "fun main() { let n = 3; print(n[Task.title]); }",
        "this key reaches into a `Task`, and the subject is a `i32`",
    );
}

// --- generic structs (R8) ---------------------------------------------------

/// A generic struct's key takes its arguments from where it lands: an
/// annotation, a parameter, and the subject of the subscript itself.
#[test]
fn keys_over_a_generic_struct_take_the_expected_arguments() {
    assert_compiles_and_runs(
        r#"
        struct Pair<T> { left: T, right: T }

        fun pick<T>(p: Pair<T>, k: Key<Pair<T>, T>): T {
            p[k]
        }

        fun pick_i32(p: Pair<i32>, k: Key<Pair<i32>, i32>): i32 {
            p[k]
        }

        fun main() {
            mut p = Pair { left = 1, right = 2 };
            let left: Key<Pair<i32>, i32> = Pair.left;
            print(p[left]);
            print(p[Pair.right]);
            print(pick(p, Pair.right));
            print(pick_i32(p, Pair.left));
            p[Pair.left] = 5;
            print(p.left);
            let s = Pair { left = "x", right = "y" };
            print(s[Pair.left]);
            print(Pair.left as Key<Pair<str>, str> == Pair.left as Key<Pair<str>, str>);
        }
        "#,
        "1\n2\n2\n1\n5\nx\ntrue\n",
    );
}

/// With nothing to take its arguments from, a generic struct's key is refused
/// where it is used: its arguments are never determined.
#[test]
fn keys_over_a_generic_struct_with_no_expectation_are_refused() {
    assert_fails_with(
        r#"
        struct Pair<T> { left: T, right: T }
        fun main() {
            print(Pair.left.name());
        }
        "#,
        "cannot infer the type of this value: it is `Key<Pair<…>, …>`",
    );
    assert_fails_with(
        r#"
        struct Pair<T> { left: T, right: T }
        fun main() {
            let k = Pair.left;
            let p = Pair { left = 1, right = 2 };
            print(p[k]);
        }
        "#,
        "this key's struct type arguments are never determined",
    );
}

/// A step past a field typed by the root's own parameter has no struct to
/// name a field of.
#[test]
fn keys_a_step_into_a_type_parameter_is_refused() {
    assert_fails_with(
        r#"
        struct Meta { owner: str }
        struct Box<T> { value: T }
        fun main() { let k = Box.value.owner; }
        "#,
        "a key reaches only struct fields: `Box.value` has type `T`, a type parameter, whose fields a key cannot name",
    );
}

// --- an ordinary value ------------------------------------------------------

/// A key is copied, stored in a struct and a list, captured by a closure,
/// passed and returned.
#[test]
fn keys_are_ordinary_values() {
    keys_runs(
        r#"
        struct Column { key: Key<Task, str>, label: str }

        fun owner_key(): Key<Task, str> {
            Task.meta.owner
        }

        fun main() {
            let t = task();
            let columns = [
                Column { key = Task.title, label = "T" },
                Column { key = owner_key(), label = "O" },
            ];
            for column in columns {
                print(column.label + "=" + t[column.key]);
            }
            let keys: List<Key<Task, str>> = [Task.status, Task.title];
            print(t[keys[0]]);
            let k = Task.status;
            let read = |x: Task| x[k];
            print(read(t));
            let copy = k;
            print(copy == k);
        }
        "#,
        "T=a\nO=ada\nopen\nopen\ntrue\n",
    );
}

/// A key keys a `HashMap` and a `HashSet`: equal paths are one entry however
/// each was written.
#[test]
fn keys_key_a_hash_map_and_a_hash_set() {
    keys_runs(
        r#"
        import std::hash_map::HashMap;
        import std::hash_set::HashSet;

        fun main() {
            mut labels: HashMap<Key<Task, str>, str> = HashMap::new();
            labels.insert(Task.title, "title");
            labels.insert(Task.meta.owner, "owner");
            print(labels.get(Task.meta.then(Meta.owner)).unwrap_or("none"));
            print(labels.get(Task.status).unwrap_or("none"));
            mut seen: HashSet<Key<Task, str>> = HashSet::new();
            seen.insert(Task.meta.owner);
            seen.insert(Task.meta.then(Meta.owner));
            print(seen.len());
        }
        "#,
        "owner\nnone\n1\n",
    );
}

// --- identity: R1 and the laws of §3.3 --------------------------------------

/// Equality, `name`, `starts_with` and `then`, through the key and through its
/// erased `KeyPath`.
#[test]
fn keys_identity_names_and_prefixes() {
    keys_runs(
        r#"
        import std::display::Display;
        import std::key::{ KeyPath, KeyStep };

        fun main() {
            let owner = Task.meta.owner;
            print(owner == Task.meta.then(Meta.owner));
            print(Task.meta.then(Meta.stars) == Task.meta.stars);
            print(owner.name());
            print(Task.title.name());
            print(owner.starts_with(Task.meta));
            print(owner.starts_with(owner));
            print(Task.meta.starts_with(owner));
            print(owner.starts_with(Task.title));
            let path: KeyPath = owner.path();
            print(path.name());
            print(path.starts_with(Task.meta.path()));
            let steps: List<KeyStep> = path.steps();
            print(steps.len());
            print(steps[0].name() + "@" + steps[0].index().to_string());
            print(steps[1].name() + "@" + steps[1].index().to_string());
            print(path == Task.meta.path().then(Meta.owner.path()));
            print(path.hash() == Task.meta.path().then(Meta.owner.path()).hash());
            print(path == Task.title.path());
        }
        "#,
        "true\ntrue\nmeta.owner\ntitle\ntrue\ntrue\nfalse\nfalse\nmeta.owner\ntrue\n2\nmeta@2\nowner@0\ntrue\ntrue\nfalse\n",
    );
}

/// K1-K5 (§3.3), each on a literal key and on keys held in values.
#[test]
fn keys_the_laws_k1_to_k5_hold() {
    keys_runs(
        r#"
        import std::display::Display;

        fun main() {
            // K1: s[k] = v; s[k] == v
            mut t = task();
            let k = Task.meta.owner;
            t[k] = "zed";
            print(t[k] == "zed");
            t[Task.meta.stars] = 4;
            print(t[Task.meta.stars] == 4);
            // K2: s[k] = s[k] changes nothing
            let before = t.title + t.status + t.meta.owner + t.meta.stars.to_string();
            t[k] = t[k];
            t[Task.meta] = t[Task.meta];
            print(before == t.title + t.status + t.meta.owner + t.meta.stars.to_string());
            // K3: keys with equal paths reach the same place
            let a = Task.meta.owner;
            let b = Task.meta.then(Meta.owner);
            print(a == b);
            t[a] = "via a";
            print(t[b]);
            // K4: keys where neither path starts with the other are disjoint
            let x = Task.meta.owner;
            let y = Task.meta.stars;
            let z = Task.title;
            print(!x.starts_with(z) && !z.starts_with(x));
            let stars = t[y];
            let title = t[z];
            t[x] = "disjoint";
            print(t[y] == stars && t[z] == title);
            // K5: a.then(b).path() == a.path().then(b.path())
            print(Task.meta.then(Meta.owner).path() == Task.meta.path().then(Meta.owner.path()));
            print(Task.meta.then(Meta.stars).path() == Task.meta.path().then(Meta.stars.path()));
        }
        "#,
        "true\ntrue\ntrue\ntrue\nvia a\ntrue\ntrue\ntrue\ntrue\n",
    );
}

/// A step carries its owner TYPE: two structs with a same-named field at the
/// same position make different steps, so their erased paths differ. A
/// generic struct's step is its declaration's, whatever the arguments.
#[test]
fn keys_a_step_carries_its_declaring_struct() {
    assert_compiles_and_runs(
        r#"
        struct Left { x: i32 }
        struct Right { x: i32 }
        struct Pair<T> { left: T, right: T }

        fun main() {
            print(Left.x.path() == Right.x.path());
            print(Left.x.name() == Right.x.name());
            let a: Key<Pair<i32>, i32> = Pair.left;
            let b: Key<Pair<str>, str> = Pair.left;
            print(a.path() == b.path());
            print(Left.x.path().steps()[0].owner() == Right.x.path().steps()[0].owner());
        }
        "#,
        "false\ntrue\ntrue\nfalse\n",
    );
}

/// A key's erased steps are a copy: changing the list `steps()` answered
/// leaves the key alone (the literal is one shared constant).
#[test]
fn keys_a_steps_list_is_a_copy() {
    keys_runs(
        r#"
        fun main() {
            mut steps = Task.meta.owner.path().steps();
            steps.pop();
            print(steps.len());
            print(Task.meta.owner.path().steps().len());
            print(Task.meta.owner.name());
        }
        "#,
        "1\n2\nmeta.owner\n",
    );
}

/// A key is a compile-time value too: `const` folds its name and a read
/// through it.
#[test]
fn keys_fold_in_a_const_expression() {
    keys_runs(
        r#"
        fun read<F>(t: Task, k: Key<Task, F>): F {
            t[k]
        }

        fun renamed(k: Key<Task, str>): Task {
            mut t = task();
            t[k] = "const";
            let v = &mut t[Task.meta.owner];
            v = *v + "!";
            t
        }

        fun main() {
            let name = const Task.meta.owner.name();
            let owner = const read(renamed(Task.meta.owner), Task.meta.owner);
            print(name);
            print(owner);
        }
        "#,
        "meta.owner\nconst!\n",
    );
}

// --- what a key cannot be ---------------------------------------------------

/// No public constructor: `Key`, `KeyPath` and `KeyStep` cannot be written as
/// literals outside std, and their insides cannot be read.
#[test]
fn keys_cannot_be_forged_or_opened() {
    keys_refused(
        "fun main() { let k = Key<Task, str> { erased = Task.title.path() }; }",
        "`erased` is an `[internal]` field of std's `Key`, and a struct literal outside std cannot write an internal std field",
    );
    keys_refused(
        "import std::key::KeyPath; fun main() { let p = KeyPath { parts = [] }; }",
        "`parts` is an `[internal]` field of std's `KeyPath`, and a struct literal outside std cannot write an internal std field",
    );
    keys_refused(
        "import std::key::KeyStep; fun main() { let s = KeyStep { owner = \"x\", index = 0, name = \"a\" }; }",
        "`owner` is an `[internal]` field of std's `KeyStep`, and a struct literal outside std cannot write an internal std field",
    );
    keys_refused(
        "fun main() { let k = Task.title; print(k.erased.name()); }",
        "`erased` is an `[internal]` field of std's `Key`, and an internal std field is not a member outside std",
    );
    keys_refused(
        "fun main() { print(Task.title.path().parts.len()); }",
        "`parts` is an `[internal]` field of std's `KeyPath`, and an internal std field is not a member outside std",
    );
    // A member written after a key LITERAL continues its path, so it names a
    // field of the field type, never a member of `Key`.
    keys_refused(
        "fun main() { print(Task.title.erased); }",
        "a key reaches only struct fields: `Task.title` has type `str`, which has no fields a key can name",
    );
}

/// A field std marks `[internal]` has no key outside std, as it is no member.
#[test]
fn keys_an_internal_std_field_has_no_key() {
    assert_fails_with(
        "import std::key::KeyPath; fun main() { let k = KeyPath.parts; }",
        "`parts` is an `[internal]` field of std's `KeyPath`, and an internal std field is not a member outside std, so it has no key",
    );
}

/// An unknown field is refused with the struct's name and a near spelling.
#[test]
fn keys_an_unknown_field_is_refused_with_a_suggestion() {
    keys_refused(
        "fun main() { let k = Task.titl; }",
        "struct 'Task' has no field 'titl'; did you mean `title`?",
    );
    keys_refused(
        "fun main() { let k = Task.meta.ownr; }",
        "struct 'Meta' has no field 'ownr'; did you mean `owner`?",
    );
}

/// S3's places are refused: a key reaches only struct fields, never into an
/// `Option`, a list, a map, a tuple or a scalar.
#[test]
fn keys_a_step_past_a_struct_field_is_refused() {
    let shapes = r#"
        import std::hash_map::HashMap;
        struct Inner { v: i32 }
        struct Doc {
            title: Option<str>,
            cover: Option<Inner>,
            tags: List<str>,
            scores: HashMap<str, i32>,
            at: (i32, i32),
            n: i32,
        }
    "#;
    assert_fails_with(
        &format!("{shapes} fun main() {{ let k = Doc.cover.v; }}"),
        "a key reaches only struct fields: `Doc.cover` has type `Option<Inner>`, and a step into an `Option` or an enum's variant may find nothing there",
    );
    assert_fails_with(
        &format!("{shapes} fun main() {{ let k = Doc.tags.len; }}"),
        "a key reaches only struct fields: `Doc.tags` has type `List<str>`, which has no fields a key can name",
    );
    assert_fails_with(
        &format!("{shapes} fun main() {{ let k = Doc.scores.size; }}"),
        // A std struct's internal fields are no members, and so have no keys.
        "struct 'HashMap' has no field 'size'",
    );
    assert_fails_with(
        &format!("{shapes} fun main() {{ let k = Doc.at.0; }}"),
        "a key reaches only struct fields: `Doc.at` has type `(i32, i32)`, and a key does not step into a tuple's positions",
    );
    assert_fails_with(
        &format!("{shapes} fun main() {{ let k = Doc.n.x; }}"),
        "a key reaches only struct fields: `Doc.n` has type `i32`, which has no fields a key can name",
    );
    // A key TO an optional field is total and fine: the field is always there.
    assert_compiles_and_runs(
        &format!(
            "{shapes} fun main() {{ mut d = Doc {{ title = None, cover = None, tags = [], \
             scores = HashMap::new(), at = (1, 2), n = 0 }}; d[Doc.title] = Some(\"t\"); \
             print(d.title.unwrap_or(\"none\")); print(d[Doc.at].1); }}"
        ),
        "t\n2\n",
    );
}

/// A resource and an external type have no fields a key may name.
#[test]
fn keys_into_a_resource_or_an_external_type_are_refused() {
    assert_fails_with(
        "[resource] struct Handle { fd: i32 } fun main() { let k = Handle.fd; }",
        "`Handle` is a resource, and a key cannot reach into a resource's fields",
    );
    assert_fails_with(
        "external struct Host; fun main() { let k = Host.x; }",
        "a key reaches only struct fields: `Host` is an external type, which has no fields a key can name",
    );
}

/// A type that is not a struct is not a key root: an enum's name before a
/// member stays the error it was.
#[test]
fn keys_an_enum_name_is_no_key_root() {
    assert_fails_with(
        "enum Mode { On, Off } fun main() { let k = Mode.On; }",
        "`Mode` is a type, not a value; use one of its variants",
    );
}

// --- emission ---------------------------------------------------------------

/// A key literal is one hoisted constant however often it is written, and a
/// subscript through a literal is the plain field chain.
#[test]
fn keys_a_literal_is_one_hoisted_constant_and_a_field_chain() {
    let source = format!(
        "{KEYS_PRELUDE}\n\
         fun main() {{\n\
         \tmut t = task();\n\
         \tfor _ in [1, 2, 3] {{ print(Task.meta.owner.name()); }}\n\
         \tprint(Task.meta.owner == Task.meta.owner);\n\
         \tt[Task.meta.owner] = \"b\";\n\
         \tprint(t[Task.meta.owner]);\n\
         }}\n"
    );
    let js = compile(&source).expect("compiles");
    assert_eq!(
        js.matches("2, \"meta\" ], [").count(),
        1,
        "one constant for the literal:\n{js}"
    );
    assert!(
        js.contains("t[2][0] = \"b\""),
        "a literal write is the field chain:\n{js}"
    );
    assert!(
        !js.contains("__key_at") && !js.contains("__key_put"),
        "no walk for a literal key:\n{js}"
    );
}
