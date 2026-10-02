//! A142 S7 — the `Store` (`proposal/store.md`, RULED 2026-10-01).
//!
//! A `Store<T>` is a root plus a static path: the root's value lives in one
//! `Shared`, in place, and a projection (`user.address().city()`) is another
//! handle cut from its parent whose `set` writes back through the root.
//! `[derive(Storable)]` emits the per-type diff and the projections. A slot
//! (one path's subscriber list) is made by the first subscription at its path
//! and goes with the last; a write wakes exactly the live slots whose value
//! changed, and a handle write diffs only its own subtree, then wakes the
//! ancestors' whole-value slots (the spine).
//!
//! The pins here follow the paper's prototype (`store_lens.vl`'s scenarios,
//! Appendix A): the woken lists, the comparison counts where a counting
//! `PartialEq` makes them observable, and the slot census.

use crate::support::*;

/// The paper's toy root, derived, with a log the observers write into. A
/// `Counted` leaf counts its comparisons, which is how the pins see what a
/// write compared.
const TYPES: &str = r#"
import std::compare::PartialEq;
import std::io::print;
import std::reactive::{ Owner, Signal, SignalCell, Source, batch, derive, run_with_owner };
import std::shared::Shared;
import std::store::{ Storable, Store, StoreFlag, StoreSome, store_census };

let compares = Shared::new(0);
let woke = Shared::new([""]);

struct Counted {
    value: i32,
}

impl Counted with PartialEq {
    fun eq(self, other: Counted): bool {
        compares.write() += 1;
        self.value == other.value
    }
}

[derive(PartialEq, Storable)]
struct Address {
    city: str,
    zip: str,
}

[derive(Storable)]
struct User {
    name: str,
    age: Counted,
    address: Address,
    nick: Option<str>,
}

fun alice(): User {
    User {
        name = "Alice",
        age = Counted { value = 30 },
        address = Address { city = "Oslo", zip = "0150" },
        nick = None,
    }
}

fun mark(name: str) {
    woke.write().push(name);
}

/// What woke since the last call, and the comparisons a `Counted` made.
fun drained(): str {
    mut out = "";
    for name in woke.read() {
        if out != "" {
            out = out + " ";
        }
        out = out + name;
    }
    woke.write() = [];
    let counted = compares.read();
    compares.write() = 0;
    if out == "" { i"[-] compares={counted}" } else { i"[{out}] compares={counted}" }
}

fun census<T>(store: Store<T>): str {
    let (slots, nodes) = store_census(store);
    i"slots={slots} nodes={nodes}"
}
"#;

fn program(body: &str) -> String {
    format!("{TYPES}\nfun main() {{\n{body}\n}}\n\nmain();\n")
}

#[test]
fn a142_s7_a_read_allocates_nothing_and_a_slot_lives_as_long_as_its_subscriptions() {
    // Q12: constructing a store allocates the root node and nothing else; a read
    // allocates nothing; a subscription allocates its slot and the nodes down to
    // it; two subscriptions on one path share one slot, and the last release takes
    // the slot and every node it left empty. Red when a released slot stays
    // (`slots=1 nodes=3` at the end), or a read allocates (`slots=0 nodes=3`).
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            let _city = user.address().city().get();
            print(i"new + one read: {census(user)}");
            let first = user.address().city().on_change(|c| mark(c));
            let second = user.address().city().on_change(|c| mark(c));
            print(i"two on city: {census(user)}");
            let third = user.name().on_change(|n| mark(n));
            print(i"and one on name: {census(user)}");
            first.dispose();
            print(i"one on city left: {census(user)}");
            second.dispose();
            print(i"none on city: {census(user)}");
            third.dispose();
            print(i"nothing watched: {census(user)}");
            "#,
        ),
        "new + one read: slots=0 nodes=1\n\
         two on city: slots=1 nodes=3\n\
         and one on name: slots=2 nodes=4\n\
         one on city left: slots=2 nodes=4\n\
         none on city: slots=1 nodes=2\n\
         nothing watched: slots=0 nodes=1\n",
    );
}

#[test]
fn a142_s7_a_whole_write_wakes_exactly_the_changed_slots_and_their_ancestors() {
    // §2.3: `zip` changed wakes `address.zip`, `address` and the root, and never
    // `name`, `address.city` or `age`. A whole write of the value already held
    // wakes nothing. Red when a whole write wakes every slot (the coarse cell).
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            let watching = Owner::new();
            run_with_owner(watching, || {
                user.name().effect_on_change(|_n| mark("name"));
                user.age().effect_on_change(|_a| mark("age"));
                user.address().city().effect_on_change(|_c| mark("address.city"));
                user.address().zip().effect_on_change(|_z| mark("address.zip"));
                user.address().effect_on_change(|_a| mark("address"));
                user.effect_on_change(|_u| mark("user"));
            });
            mut next = alice();
            next.address.zip = "0151";
            user.set(next);
            print(i"zip changed: {drained()}");
            user.set(next);
            print(i"the same value again: {drained()}");
            next.name = "Alicia";
            next.age = Counted { value = 31 };
            user.set(next);
            print(i"name and age changed: {drained()}");
            watching.dispose();
            "#,
        ),
        "zip changed: [address.zip address user] compares=1\n\
         the same value again: [-] compares=1\n\
         name and age changed: [name age user] compares=1\n",
    );
}

#[test]
fn a142_s7_a_whole_write_with_no_live_slot_compares_nothing() {
    // §2.3: with no slot live, a whole write costs no comparison at all — the
    // `need` flag reaches no leaf. Red when the diff walks an unwatched store
    // (`compares=1` per write).
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            mut next = alice();
            mut i = 0;
            for i < 5 {
                next.age = Counted { value = i };
                user.set(next);
                i += 1;
            }
            print(i"five whole writes, nothing watched: {drained()} age={user.age().get().value}");
            "#,
        ),
        "five whole writes, nothing watched: [-] compares=0 age=4\n",
    );
}

#[test]
fn a142_s7_a_handle_write_commits_along_the_spine_and_never_looks_at_a_sibling() {
    // §2.5: `city.set` assigns in place through the root, diffs only `city`, and
    // wakes `city`, then `address` and the root. `age` (a sibling with a live slot)
    // is never compared. The same value again wakes nothing. Red when a handle
    // write diffs from the root (`compares=1` on the city write: `age` compared).
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            let watching = Owner::new();
            run_with_owner(watching, || {
                user.age().effect_on_change(|_a| mark("age"));
                user.address().city().effect_on_change(|_c| mark("address.city"));
                user.address().zip().effect_on_change(|_z| mark("address.zip"));
                user.address().effect_on_change(|_a| mark("address"));
                user.effect_on_change(|_u| mark("user"));
            });
            let city = user.address().city();
            city.set("Bergen");
            let now: User = user.get();
            print(i"city set: {drained()} root.city={now.address.city}");
            city.set("Bergen");
            print(i"the same value: {drained()}");
            let copy = city;
            copy.set("Trondheim");
            print(i"through a copy of the handle: {drained()} city={city.get()}");
            user.age().set(Counted { value = 31 });
            print(i"age set: {drained()}");
            watching.dispose();
            "#,
        ),
        "city set: [address.city address user] compares=0 root.city=Bergen\n\
         the same value: [-] compares=0\n\
         through a copy of the handle: [address.city address user] compares=0 city=Trondheim\n\
         age set: [age user] compares=1\n",
    );
}

#[test]
fn a142_s7_notify_wakes_the_slot_and_its_ancestors_without_comparing() {
    // Q7's escape hatch: `notify()` wakes the path's slot and every ancestor's
    // whole-value slot, compares nothing, and leaves siblings alone.
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            let watching = Owner::new();
            run_with_owner(watching, || {
                user.name().effect_on_change(|_n| mark("name"));
                user.address().city().effect_on_change(|_c| mark("address.city"));
                user.address().effect_on_change(|_a| mark("address"));
                user.effect_on_change(|_u| mark("user"));
            });
            user.address().city().notify();
            print(i"city notified: {drained()}");
            user.age().notify();
            print(i"age notified (no slot of its own): {drained()}");
            watching.dispose();
            "#,
        ),
        "city notified: [address.city address user] compares=0\n\
         age notified (no slot of its own): [user] compares=0\n",
    );
}

#[test]
fn a142_s7_a_store_handle_is_a_source_and_a_signal_to_the_whole_reactive_layer() {
    // §1.1, re-checked on the built handle: a pipe over a projection follows it,
    // `set_with` (the trait default) writes through it, a generic `S: Signal<T>`
    // takes one, and two writes in one `batch` are one effect run. A write that
    // changes two leaves wakes a tracked derivation of both ONCE: the store's
    // wakes are delivered by one turn, which dedups the shared subscriber.
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            let city = user.address().city();
            let zip = user.address().zip();
            let runs = Shared::new(0);
            let watching = Owner::new();
            run_with_owner(watching, || {
                let shout = city.derive(|c| c + "!").memo();
                shout.effect(|s| print(i"shout={s}"));
                derive(|| city.track() + " " + zip.track())
                    .effect_on_change(|line| {
                        runs.write() += 1;
                        print(i"line={line}");
                    });
            });
            city.set_with(|c| c + " S");
            batch(|| {
                city.set("Bergen");
                city.set("Bergen N");
            });
            mut next = user.get();
            next.address = Address { city = "Molde", zip = "6400" };
            user.set(next);
            print(i"line runs={runs.read()}");
            write_through(zip, "6401");
            watching.dispose();
            "#,
        )
        .replace(
            "fun main() {",
            "fun write_through<S: Signal<str>>(signal: S, value: str) {\n    signal.set(value);\n}\n\nfun main() {",
        ),
        "shout=Oslo!\n\
         shout=Oslo S!\n\
         line=Oslo S 0150\n\
         shout=Bergen N!\n\
         line=Bergen N 0150\n\
         shout=Molde!\n\
         line=Molde 6400\n\
         line runs=3\n\
         line=Molde 6401\n",
    );
}

#[test]
fn a142_s7_a_coarse_field_is_one_slot_compared_whole() {
    // Q4: `[reactive(coarse)]` diffs the field with `==` even though `Address`
    // derives `Storable` — one `Address` comparison (counted here through a
    // hand-written `PartialEq`) where the derived diff compares field by field.
    // A projection below the coarse field still wakes when the field changed.
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::io::print;
        import std::reactive::{ Owner, Signal, Source, run_with_owner };
        import std::shared::Shared;
        import std::store::{ Storable, Store };

        let compares = Shared::new(0);

        [derive(Storable)]
        struct Address {
            city: str,
            zip: str,
        }

        impl Address with PartialEq {
            fun eq(self, other: Address): bool {
                compares.write() += 1;
                self.city == other.city && self.zip == other.zip
            }
        }

        [derive(Storable)]
        struct User {
            name: str,
            [reactive(coarse)]
            address: Address,
        }

        fun main() {
            let user = Store::new(User { name = "a", address = Address { city = "Oslo", zip = "0150" } });
            let watching = Owner::new();
            run_with_owner(watching, || {
                user.address().effect_on_change(|_a| print("address woke"));
                user.address().city().effect_on_change(|_c| print("address.city woke"));
            });
            user.set(User { name = "a", address = Address { city = "Oslo", zip = "0151" } });
            print(i"zip changed: compares={compares.read()}");
            compares.write() = 0;
            user.set(User { name = "a", address = Address { city = "Oslo", zip = "0151" } });
            print(i"the same again: compares={compares.read()}");
            watching.dispose();
        }

        main();
        "#,
        "address woke\naddress.city woke\nzip changed: compares=1\nthe same again: compares=1\n",
    );
}

#[test]
fn a142_s7_a_renamed_projection_takes_the_name_the_attribute_gives() {
    // Q9's way out: `[reactive(name = "..")]` generates the projection under
    // another name, so a field may be called `get`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Source };
        import std::store::{ Storable, Store };

        [derive(Storable)]
        struct Request {
            [reactive(name = "verb")]
            get: str,
            [reactive(coarse, name = "headers_all")]
            headers: List<str>,
        }

        fun main() {
            let request = Store::new(Request { get = "/", headers = ["a"] });
            request.verb().set("/index");
            print(request.verb().get());
            print(request.headers_all().get().len());
        }

        main();
        "#,
        "/index\n1\n",
    );
}

#[test]
fn a142_s7_the_derive_refuses_a_field_named_like_a_handle_member() {
    // Q9: an inherent member wins over a generated projection, so a field called
    // `get`, `set`, `derive`, `patch`, ... could never be reached as a projection;
    // the derive refuses it and steers to `[reactive(name = "..")]`.
    for member in ["get", "set", "derive", "effect", "patch", "some", "live"] {
        assert_fails_with(
            &format!(
                r#"
                import std::store::Storable;

                [derive(Storable)]
                struct Clash {{
                    {member}: i32,
                }}

                fun main() {{}}
                "#
            ),
            &format!("would project as `{member}()`"),
        );
    }
}

#[test]
fn a142_s7_a_reactive_attribute_takes_only_its_two_arguments() {
    // The parser commits at `[reactive`: an unknown argument, a `name` that is not
    // an identifier, and a bare `name` are each refused where they stand.
    assert_fails_with(
        "struct S {\n    [reactive(fine)]\n    a: i32,\n}\n",
        "`[reactive(..)]` on a field takes `coarse`",
    );
    assert_fails_with(
        "struct S {\n    [reactive(name = \"1st\")]\n    a: i32,\n}\n",
        "so it must be an identifier",
    );
    assert_fails_with(
        "struct S {\n    [reactive(name)]\n    a: i32,\n}\n",
        "`[reactive(..)]` on a field takes `coarse`",
    );
    // Both orders around `[expose]` parse.
    assert_compiles(
        "import std::reactive::SignalCell;\n\nstruct S {\n    [reactive(coarse)] [expose] a: SignalCell<i32>,\n    [expose] [reactive(coarse)] b: SignalCell<i32>,\n}\n\nfun main() {}\n",
    );
}

#[test]
fn a142_s7_a_leaf_with_no_equality_changes_on_every_covering_write() {
    // §2.3's bare tier: a closure-typed field has no `PartialEq`, so every write
    // that covers it wakes its slot.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, Source, run_with_owner };
        import std::store::{ Storable, Store };

        [derive(Storable)]
        struct Button {
            label: str,
            on_press: || void,
        }

        fun nothing() {}

        fun main() {
            let button = Store::new(Button { label = "ok", on_press = nothing });
            let watching = Owner::new();
            run_with_owner(watching, || {
                button.on_press().effect_on_change(|_f| print("on_press woke"));
                button.label().effect_on_change(|_l| print("label woke"));
            });
            button.set(Button { label = "ok", on_press = nothing });
            button.set(Button { label = "ok", on_press = nothing });
            watching.dispose();
        }

        main();
        "#,
        "on_press woke\non_press woke\n",
    );
}

#[test]
fn a142_s7_an_option_field_is_reached_through_its_some() {
    // §4: `Option` is structural with no derive. `nick().some()` is a
    // `StoreSome<str>` — `None` while the option is `None` — and `patch` writes
    // only while it is `Some`. `is_some()` is the discriminant.
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            let nick = user.nick().some();
            let watching = Owner::new();
            run_with_owner(watching, || {
                nick.effect_on_change(|n| mark(i"nick={n.unwrap_or("-")}"));
                user.nick().is_some().effect_on_change(|s| mark(i"is_some={s}"));
            });
            print(i"patch while None: landed={nick.patch("al")} {drained()}");
            user.nick().set(Some("ally"));
            print(i"None -> Some: {drained()}");
            print(i"patch while Some: landed={nick.patch("al")} {drained()}");
            user.nick().set(None);
            print(i"Some -> None: {drained()} read={nick.get().unwrap_or("-")}");
            watching.dispose();
            "#,
        ),
        "patch while None: landed=false [-] compares=0\n\
         None -> Some: [nick=ally is_some=true] compares=0\n\
         patch while Some: landed=true [nick=al] compares=0\n\
         Some -> None: [nick=- is_some=false] compares=0 read=-\n",
    );
}

#[test]
fn a142_s7_a_generic_struct_derives_and_projects_its_parameters() {
    // B194's binder rule through the derive: `Pair<T>`'s impl binds `T` under
    // `Storable`, and its projections are `Store<T>`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, Source, run_with_owner };
        import std::store::{ Storable, Store };

        [derive(Storable)]
        struct Pair<T> {
            left: T,
            right: T,
        }

        fun main() {
            let pair = Store::new(Pair { left = 1, right = 2 });
            let watching = Owner::new();
            run_with_owner(watching, || {
                pair.left().effect_on_change(|l| print(i"left={l}"));
                pair.right().effect_on_change(|r| print(i"right={r}"));
            });
            pair.set(Pair { left = 1, right = 3 });
            pair.left().set(5);
            watching.dispose();
        }

        main();
        "#,
        "right=3\nleft=5\n",
    );
}

#[test]
fn a142_s7_a_handle_reads_one_leaf_without_copying_the_root() {
    // §3.1's measurement as a pin: the read LENDS in place, so a leaf read
    // copies the leaf and never the root. A `Counted` root field would be copied
    // by a value-composed read; here the observable is that a read through a
    // projection of a large root allocates nothing (the census) and answers the
    // leaf. (The timing — 2,000 reads on a 1,000-key root — is in the lane
    // report, measured on both backends.)
    assert_compiles_and_runs(
        r#"
        import std::hash_map::HashMap;
        import std::io::print;
        import std::reactive::Source;
        import std::store::{ Storable, Store, store_census };

        [derive(Storable)]
        struct Root {
            label: str,
            scores: HashMap<str, i32>,
        }

        fun main() {
            mut scores: HashMap<str, i32> = HashMap::new();
            mut i = 0;
            for i < 1000 {
                scores.insert(i"k{i}", i);
                i += 1;
            }
            let root = Store::new(Root { label = "hi", scores });
            let label = root.label();
            mut total = 0;
            i = 0;
            for i < 2000 {
                total += label.get().len().as_i32();
                i += 1;
            }
            let (slots, nodes) = store_census(root);
            print(i"chars={total} slots={slots} nodes={nodes}");
        }

        main();
        "#,
        "chars=4000 slots=0 nodes=1\n",
    );
}
