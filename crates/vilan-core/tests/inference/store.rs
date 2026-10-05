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
import std::reactive::store::{ Storable, Store, StoreFlag, StoreSome, store_census };

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
        import std::reactive::store::{ Storable, Store };

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
        import std::reactive::store::{ Storable, Store };

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
                import std::reactive::store::Storable;

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
        import std::reactive::store::{ Storable, Store };

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
        import std::reactive::store::{ Storable, Store };

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
        import std::reactive::store::{ Storable, Store, store_census };

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

// ── S2: enums ───────────────────────────────────────────────────────────────

/// The paper's `Presence` (Appendix A) beside `TYPES`' leaves: an enum whose
/// payload is a derived struct, so a handle reaches through the variant.
const VARIANTS: &str = r#"
[derive(PartialEq, Storable)]
struct Device {
    name: str,
    since: Counted,
}

[derive(PartialEq, Storable)]
enum Presence {
    Offline,
    Online(Device),
    Away(str, i32),
}

fun online(name: str, since: i32): Presence {
    Presence::Online(Device { name, since = Counted { value = since } })
}
"#;

fn variant_program(body: &str) -> String {
    format!("{TYPES}\n{VARIANTS}\nfun main() {{\n{body}\n}}\n\nmain();\n")
}

#[test]
fn a142_s7_a_same_variant_write_patches_the_payload_and_rebuilds_nothing() {
    // §2.6: `Online(d1)` to `Online(d2)` with only `since` changed wakes `since`
    // and the enum's own slot — not `name`, not the discriminant — so a consumer
    // gated on the discriminant builds once across a hundred such writes. Red
    // when a same-variant write is treated as a variant switch
    // (`tag=100 name=100`).
    assert_compiles_and_runs(
        &variant_program(
            r#"
            let presence = Store::new(online("laptop", 0));
            let tags = Shared::new(0);
            let names = Shared::new(0);
            let sinces = Shared::new(0);
            let watching = Owner::new();
            run_with_owner(watching, || {
                presence.is_online().effect_on_change(|_t| tags.write() += 1);
                presence.online().name().effect_on_change(|_n| names.write() += 1);
                presence.online().since().effect_on_change(|_s| sinces.write() += 1);
                presence.effect_on_change(|_p| mark("presence"));
            });
            mut i = 1;
            for i <= 100 {
                presence.set(online("laptop", i));
                i += 1;
            }
            woke.write() = [];
            print(i"100 same-variant writes: tag={tags.read()} name={names.read()} since={sinces.read()} {drained()}");
            watching.dispose();
            "#,
        ),
        "100 same-variant writes: tag=0 name=0 since=100 [-] compares=100\n",
    );
}

#[test]
fn a142_s7_a_variant_switch_wakes_the_discriminant_and_every_live_payload_slot() {
    // §2.6: a different variant moves every live slot under the payload between
    // `Some` and `None`, so all of them wake, and so do the two variants' flags —
    // `is_away()` stays asleep through `Online` <-> `Offline`, since its answer
    // does not move (Q7). A
    // `patch` through the dead variant lands nothing, wakes nothing, answers
    // `false` and leaves the variant as it was; switching back wakes the same
    // five; a `patch` through the live one wakes its leaf and the spine.
    assert_compiles_and_runs(
        &variant_program(
            r#"
            let presence = Store::new(online("laptop", 1));
            let name = presence.online().name();
            let watching = Owner::new();
            run_with_owner(watching, || {
                presence.is_online().effect_on_change(|t| mark(i"tag={t}"));
                name.effect_on_change(|n| mark(i"name={n.unwrap_or("-")}"));
                presence.online().since().effect_on_change(|s| mark(i"since={s.is_some()}"));
                presence.is_away().effect_on_change(|a| mark(i"away={a}"));
                presence.effect_on_change(|_p| mark("presence"));
            });
            presence.set(Presence::Offline);
            print(i"to Offline: {drained()}");
            print(i"patch through the dead variant: landed={name.patch("ghost")} {drained()} online={presence.is_online().get()}");
            presence.set(online("phone", 5));
            print(i"back Online: {drained()}");
            print(i"patch through the live variant: landed={name.patch("tablet")} {drained()}");
            presence.set(Presence::Away("lunch", 30));
            print(i"to Away: {drained()} away={presence.away().get().unwrap_or(("-", 0)).0}");
            watching.dispose();
            "#,
        ),
        "to Offline: [tag=false name=- since=false presence] compares=0\n\
         patch through the dead variant: landed=false [-] compares=0 online=false\n\
         back Online: [tag=true name=phone since=true presence] compares=0\n\
         patch through the live variant: landed=true [name=tablet presence] compares=0\n\
         to Away: [tag=false name=- since=false away=true presence] compares=0 away=lunch\n",
    );
}

#[test]
fn a142_s7_a_handle_into_a_payload_is_a_source_of_an_option_with_patch() {
    // Q6: a through-variant handle reads `None` while the variant is not live
    // and `Some` while it is; `live()` is its variant's discriminant; it projects
    // further (`online().name()`), and a multi-payload variant's handle reads the
    // payload as a tuple.
    assert_compiles_and_runs(
        &variant_program(
            r#"
            let presence = Store::new(Presence::Offline);
            let device = presence.online();
            print(i"offline: device={device.get().is_some()} live={device.live().get()} name={device.name().get().unwrap_or("-")}");
            presence.set(online("laptop", 7));
            let now: Device = device.get().unwrap();
            print(i"online: name={now.name} since={device.since().get().unwrap().value} live={device.live().get()}");
            presence.set(Presence::Away("lunch", 30));
            let (why, minutes) = presence.away().get().unwrap();
            print(i"away: {why} {minutes} online={presence.is_online().get()} offline={presence.is_offline().get()}");
            let _patched = presence.away().patch(("meeting", 60));
            let (why_now, minutes_now) = presence.away().get().unwrap();
            print(i"patched: {why_now} {minutes_now}");
            "#,
        ),
        "offline: device=false live=false name=-\n\
         online: name=laptop since=7 live=true\n\
         away: lunch 30 online=false offline=false\n\
         patched: meeting 60\n",
    );
}

#[test]
fn a142_s7_a_payload_store_assumed_reads_the_last_payload_once_the_variant_ends() {
    // `assume()` — what `when_live` hands its body — is a `Store<P>` through the
    // variant: it reads and writes the live payload, and should a reader run in
    // the turn that ends the variant it reads the LAST payload a read through the
    // live variant saw (B526: `phone`, not the `laptop` it was assumed at), while
    // a write then lands nowhere. Assuming a dead variant is refused.
    assert_compiles_and_runs(
        &variant_program(
            r#"
            let presence = Store::new(online("laptop", 1));
            let device: Store<Device> = presence.online().assume();
            device.name().set("phone");
            let held: Presence = presence.get();
            let now = match held {
                Presence::Online(let d) => d.name,
                _ => "-",
            };
            print(i"live: {device.name().get()} root={now}");
            presence.set(Presence::Offline);
            device.name().set("ghost");
            print(i"ended: {device.name().get()} online={presence.is_online().get()}");
            "#,
        ),
        "live: phone root=phone\nended: phone online=false\n",
    );
}

#[test]
fn b526_an_assumed_handle_holds_the_last_value_its_subscribers_read() {
    // B526 (mirrored-store.md S0, Q11): the payload is patched through ANOTHER
    // handle and then deleted. The assumed handle's subscriber is woken by the
    // patch and reads `edited` through the live variant; when the delete ends the
    // variant, the same subscriber wakes again and reads the fallback — which is
    // that last read, not the `hello` the handle was assumed at. Before the fix it
    // repainted `hello` for the turn before teardown.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;
        import std::io::print;
        import std::reactive::{ Flow, Owner, Source, run_with_owner };
        import std::reactive::store::{ Storable, Store, StoreSome };

        [derive(Storable, PartialEq)]
        struct Message {
            id: u53,
            content: str,
        }

        fun main() {
            let slot: Store<Option<Message>> = Store::new(Some(Message { id = 7, content = "hello" }));
            let held: Store<Message> = slot.some().assume();
            mut seen: List<str> = [];
            let owner = Owner::new();
            run_with_owner(owner, || {
                held.content().effect_on_change(|content| seen.push(content));
            });
            let _patched = slot.some().content().patch("edited");
            slot.set(None);
            print(seen.join(" "));
            print(held.content().get());
            owner.dispose();
        }

        main();
        "#,
        "edited edited\nedited\n",
    );
}

#[test]
fn a142_s7_an_enum_derive_refuses_a_variant_named_like_a_handle_member() {
    // Q9 for variants: `Set(i32)` would project as `set()` (a payload variant's
    // through-handle) and `Some(i32)` as `some()`; both refused. A bare variant
    // projects only `is_..()`, so a bare `Get` is fine.
    for variant in ["Set(i32)", "Some(i32)", "Patch(str)"] {
        assert_fails_with(
            &format!(
                r#"
                import std::reactive::store::Storable;

                [derive(Storable)]
                enum Clash {{
                    Plain,
                    {variant},
                }}

                fun main() {{}}
                "#
            ),
            "which a store handle already has",
        );
    }
    assert_compiles(
        r#"
        import std::reactive::store::Storable;

        [derive(Storable)]
        enum Fine {
            Get,
            Done(i32),
        }

        fun main() {}
        "#,
    );
}

#[test]
fn a142_s7_a_generic_enum_derives_with_a_scalar_payload() {
    // B194's binder rule through the enum derive, with a scalar `T`: the payload
    // binder is lent in place (`f(&p0)`, B504) and written through a `mut` binder
    // (`Just(mut p0)`, F79).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Source };
        import std::reactive::store::{ Storable, Store };

        [derive(Storable)]
        enum Maybe<T> {
            Nothing,
            Just(T),
        }

        fun main() {
            let maybe = Store::new(Maybe::Just(1));
            print(maybe.just().get().unwrap_or(0));
            print(maybe.just().patch(2));
            print(maybe.just().get().unwrap_or(0));
            maybe.set(Maybe::Nothing);
            print(maybe.just().patch(3));
            print(maybe.is_nothing().get());
        }

        main();
        "#,
        "1\ntrue\n2\nfalse\ntrue\n",
    );
}

#[test]
fn a142_s7_when_live_compiles_on_both_ui_layers() {
    // S2's UI helper: `when_live(handle, |payload| body)` is a `when` over the
    // handle's discriminant whose body is handed the payload's `Store<P>`. It is
    // declared on both twins, so one component compiles for the browser and for
    // the server, where it renders the live payload once.
    let component = r#"
        import std::io::print;
        import std::reactive::Source;
        import std::reactive::store::{ Storable, Store };
        import std::web::ui::{ View, view, when_live };

        [derive(PartialEq, Storable)]
        struct Device {
            name: str,
        }

        [derive(PartialEq, Storable)]
        enum Presence {
            Offline,
            Online(Device),
        }

        fun panel(presence: Store<Presence>): View {
            view("aside").child(when_live(presence.online(), |device| view("p").bind_text(device.name())))
        }
    "#;
    assert_compiles_browser(&format!("{component}\nfun main() {{}}\n"));
    assert_compiles_and_runs(
        &format!(
            "{component}\nimport std::web::ui::render;\n\nfun main() {{\n    \
             print(render(panel(Store::new(Presence::Online(Device {{ name = \"laptop\" }})))));\n    \
             print(render(panel(Store::new(Presence::Offline))));\n}}\n\nmain();\n"
        ),
        "<aside><p>laptop</p></aside>\n<aside></aside>\n",
    );
}

#[test]
fn b526_the_server_twins_when_live_serves_the_payload_as_it_stands() {
    // B526's server twin: `when_live` reads once, so its body is built over the
    // payload as it stands at the render — a rename patched through another
    // handle before the render is the one served.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::Source;
        import std::reactive::store::{ Storable, Store };
        import std::web::ui::{ View, render, view, when_live };

        [derive(PartialEq, Storable)]
        struct Device {
            name: str,
        }

        [derive(PartialEq, Storable)]
        enum Presence {
            Offline,
            Online(Device),
        }

        fun main() {
            let presence = Store::new(Presence::Online(Device { name = "laptop" }));
            let _renamed = presence.online().name().patch("phone");
            print(render(view("aside").child(when_live(presence.online(), |device| view("p").bind_text(device.name())))));
        }

        main();
        "#,
        "<aside><p>phone</p></aside>\n",
    );
}

// ── A146: a store handle names its state ────────────────────────────────────

#[test]
fn a142_s7_every_handle_to_one_path_of_one_store_shares_an_identity() {
    // A146 on the store: one number per path of one store — a projection made
    // twice, a copy, a flag asked twice and the `Store` `assume()` hands back
    // beside its `StoreSome` agree; another path, another flag and the same path
    // of another store differ.
    assert_compiles_and_runs(
        &variant_program(
            r#"
            let user = Store::new(alice());
            let other = Store::new(alice());
            let city = user.address().city();
            let copy = city;
            print(user.address().city().identity() == copy.identity());
            print(city.identity() == user.address().zip().identity());
            print(city.identity() == other.address().city().identity());
            print(user.identity() == user.identity());
            print(user.nick().is_some().identity() == user.nick().is_some().identity());
            let presence = Store::new(online("laptop", 1));
            print(presence.online().identity() == presence.online().assume().identity());
            print(presence.is_online().identity() == presence.is_away().identity());
            "#,
        ),
        "true\nfalse\nfalse\ntrue\ntrue\ntrue\nfalse\n",
    );
}

#[test]
fn a142_s7_a_tracked_read_of_a_store_handle_attaches_once_across_runs() {
    // A146's point: a body that `track()`s the same handle on every run keeps ONE
    // edge on it. The instrument wraps the handle and counts its attaches; with
    // `identity()` delegated to the handle's, five re-runs attach once. Red with
    // the handle's identity answering `None`: one attach per run and one more for the write (`attaches=7`).
    assert_compiles_and_runs(
        &program(
            r#"
            let user = Store::new(alice());
            let watched = Watched { inner = user.address().city(), attaches = Shared::new(0) };
            let trigger: SignalCell<i32> = Signal::new(0);
            let owner = Owner::new();
            run_with_owner(owner, || {
                derive(|| i"{trigger.track()} {watched.track()}").effect(|line| print(line));
            });
            mut step = 1;
            for step <= 5 {
                trigger.set(step);
                step += 1;
            }
            user.address().city().set("Bergen");
            print(i"attaches={watched.attaches.read()}");
            owner.dispose();
            "#,
        )
        .replace(
            "fun main() {",
            r#"import std::reactive::{ Subscriber, Subscription };

struct Watched {
    inner: Store<str>,
    attaches: Shared<i32>,
}

impl Watched with Source<str> {
    fun get(self): str {
        self.inner.get()
    }

    [must_use]
    fun on_settle(self, subscriber: Subscriber): Subscription {
        self.attaches.write() = self.attaches.read() + 1;
        self.inner.on_settle(subscriber)
    }

    fun identity(self): Option<i32> {
        self.inner.identity()
    }
}

fun main() {"#,
        ),
        "0 Oslo\n1 Oslo\n2 Oslo\n3 Oslo\n4 Oslo\n5 Oslo\n5 Bergen\nattaches=1\n",
    );
}

// ── A149 S3: collection fields are keyed and sequence nodes ────────────────

/// A store whose fields take the three declared shapes (§2.7): a map of
/// derived records (each with a list of its own), a set, and a keyed list. The
/// observers bump named counters; `tally()` prints the non-zero ones in a fixed
/// order and resets them, so a pin reads exactly how many times each reader
/// woke for one write.
const COLLECTIONS: &str = r#"
import std::compare::PartialEq;
import std::reactive::delta::{ CollFlow, MapFlow, MapOp, SeqOp, SequenceCell, SetFlow, SetOp };
import std::display::Display;
import std::hash_map::HashMap;
import std::hash_set::HashSet;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Owner, Signal, Source, run_with_owner };
import std::shared::Shared;
import std::reactive::store::{ Storable, Store, StoreSome, store_census, store_feed_census };
import std::wire::Keyed;

[derive(PartialEq, Storable)]
struct Channel {
    id: i32,
    name: str,
    messages: List<i32>,
}

[derive(PartialEq, Storable)]
struct Message {
    id: i32,
    text: str,
}

impl Message with Keyed<i32> {
    fun key(self): i32 {
        self.id
    }
}

[derive(Storable)]
struct Global {
    channels: HashMap<i32, Channel>,
    tags: HashSet<str>,
    log: List<Message>,
}

let names = Shared::new([""]);
let counts = Shared::new([0]);

fun bump(name: str) {
    mut held = names.read();
    mut tally = counts.read();
    mut index: usize = 0;
    for index < held.len() {
        if held[index] == name {
            tally[index] = tally[index] + 1;
            counts.write() = tally;
            ret;
        }
        index += 1;
    }
    held.push(name);
    tally.push(1);
    names.write() = held;
    counts.write() = tally;
}

/// The readers that woke since the last call, each with its count, in name
/// order (the order a turn delivers wakes in is not a contract, §3.3); `-` for
/// none.
fun tally(): str {
    mut out = "";
    let held = names.read();
    let tally = counts.read();
    mut printed: List<str> = [];
    mut round: usize = 0;
    for round < held.len() {
        mut best: Option<usize> = None;
        mut index: usize = 0;
        for index < held.len() {
            if tally[index] > 0 && !printed.contains(held[index]) {
                best = match best {
                    Some(let at) => if held[index] < held[at] { Some(index) } else { Some(at) },
                    None => Some(index),
                };
            }
            index += 1;
        }
        match best {
            Some(let at) => {
                printed.push(held[at]);
                out = out + i" {held[at]}={tally[at]}";
            },
            None => {},
        }
        round += 1;
    }
    mut cleared: List<i32> = [];
    for _name in held {
        cleared.push(0);
    }
    counts.write() = cleared;
    if out == "" { " -" } else { out }
}

fun channel(id: i32, name: str): Channel {
    Channel { id, name, messages = [] }
}

fun global(): Global {
    mut channels: HashMap<i32, Channel> = HashMap::new();
    mut id = 1;
    for id <= 6 {
        channels.insert(id, channel(id, i"c{id}"));
        id += 1;
    }
    Global { channels, tags = HashSet::new(), log = [] }
}

fun census<T>(store: Store<T>): str {
    let (slots, nodes) = store_census(store);
    i"slots={slots} nodes={nodes} feeds={store_feed_census(store)}"
}

fun map_ops(ops: List<MapOp<i32, Channel>>): str {
    mut out = "";
    for op in ops {
        let piece = match op {
            MapOp::Put(let key, let was, let now) => i" Put({key}, {was.map(|held| held.name).unwrap_or("-")}, {now.name})",
            MapOp::Delete(let key, let gone) => i" Delete({key}, {gone.name})",
            MapOp::Reset(let map) => i" Reset({map.len()})",
        };
        out = out + piece;
    }
    if out == "" { " -" } else { out }
}

fun set_ops(ops: List<SetOp<str>>): str {
    mut out = "";
    for op in ops {
        let piece = match op {
            SetOp::Add(let member) => i" Add({member})",
            SetOp::Remove(let member) => i" Remove({member})",
            SetOp::Reset(let set) => i" Reset({set.len()})",
        };
        out = out + piece;
    }
    if out == "" { " -" } else { out }
}

fun texts(list: List<Message>): str {
    mut out = "[";
    for message in list {
        if out != "[" {
            out = out + " ";
        }
        out = out + i"{message.id}:{message.text}";
    }
    out + "]"
}

fun seq_ops(ops: List<SeqOp<Message>>): str {
    mut out = "";
    for op in ops {
        let piece = match op {
            SeqOp::Splice(let at, let removed, let inserted) => i" Splice({at}, {texts(removed)}, {texts(inserted)})",
            SeqOp::SetAt(let at, let was, let now) => i" SetAt({at}, {was.text}, {now.text})",
            SeqOp::Reset(let list) => i" Reset({texts(list)})",
            SeqOp::Move(let from, let count, let to) => i" Move({from}, {count}, {to})",
        };
        out = out + piece;
    }
    if out == "" { " -" } else { out }
}
"#;

fn collections_program(body: &str) -> String {
    format!("{COLLECTIONS}\nfun main() {{\n{body}\n}}\n\nmain();\n")
}

#[test]
fn a149_s3_a_write_to_one_key_wakes_that_keys_readers_and_nobody_elses() {
    // §2.7: a map field is a keyed node. Six keys are watched one by one, the map
    // whole, and key 3's name inside its record. A write to key 3 — however it is
    // spelled — wakes key 3's readers and the map, and NOBODY else; a write of the
    // value held wakes nothing; a whole write of the map, or of the root, wakes
    // only the keys whose value changed. Red on the base: `at` does not exist (a
    // map field was a leaf, and one key's write woke every reader of the map).
    assert_compiles_and_runs(
        &collections_program(
            r#"
            let store = Store::new(global());
            let owner = Owner::new();
            run_with_owner(owner, || {
                mut id = 1;
                for id <= 6 {
                    let name = i"k{id}";
                    store.channels().at(id).effect_on_change(|_held| bump(name));
                    id += 1;
                }
                store.channels().effect_on_change(|_map| bump("map"));
                store.channels().at(3).some().name().effect_on_change(|_name| bump("name3"));
                store.tags().effect_on_change(|_tags| bump("tags"));
            });
            print(i"subscribed:{tally()}");
            let _renamed = store.channels().at(3).some().name().patch("three");
            print(i"key 3's name patched:{tally()}");
            let _pushed = store.channels().at(3).some().messages().push(30);
            print(i"key 3's list pushed:{tally()}");
            store.channels().at(3).set(store.channels().at(3).get());
            print(i"key 3 set to what it holds:{tally()}");
            store.channels().insert(4, channel(4, "four"));
            print(i"key 4 replaced:{tally()}");
            store.channels().insert(9, channel(9, "nine"));
            print(i"key 9 arrived:{tally()}");
            store.channels().remove(5);
            print(i"key 5 removed:{tally()}");
            store.channels().remove(5);
            print(i"key 5 removed again:{tally()}");
            mut whole = store.channels().get();
            whole.insert(2, channel(2, "two"));
            store.channels().set(whole);
            print(i"whole map, key 2 changed:{tally()}");
            store.channels().set(store.channels().get());
            print(i"whole map unchanged:{tally()}");
            mut root = store.get();
            root.tags.insert("new");
            store.set(root);
            print(i"root written, only the set changed:{tally()}");
            print(store.channels().at(3).get().map(|held| held.name).unwrap_or("-"));
            print(store.channels().at(5).get().is_none());
            owner.dispose();
            print(census(store));
            "#,
        ),
        "subscribed: -\n\
         key 3's name patched: k3=1 map=1 name3=1\n\
         key 3's list pushed: k3=1 map=1\n\
         key 3 set to what it holds: -\n\
         key 4 replaced: k4=1 map=1\n\
         key 9 arrived: map=1\n\
         key 5 removed: k5=1 map=1\n\
         key 5 removed again: -\n\
         whole map, key 2 changed: k2=1 map=1\n\
         whole map unchanged: -\n\
         root written, only the set changed: tags=1\n\
         three\n\
         true\n\
         slots=0 nodes=1 feeds=0\n",
    );
}

#[test]
fn a149_s3_a_keyed_slot_lives_as_long_as_its_subscriptions() {
    // Q12 on a keyed node: `at(key)` allocates nothing until something
    // subscribes; each watched key is one node with one slot under the map's
    // node; two subscriptions on one key share it; the last release takes the
    // key's node, and the map's node with it once no key is left.
    assert_compiles_and_runs(
        &collections_program(
            r#"
            let store = Store::new(global());
            let _read = store.channels().at(2).get();
            print(i"one read: {census(store)}");
            let first = store.channels().at(2).on_change(|_held| bump("a"));
            let second = store.channels().at(2).on_change(|_held| bump("b"));
            print(i"two on key 2: {census(store)}");
            let third = store.channels().at(4).some().name().on_change(|_name| bump("c"));
            print(i"and key 4's name: {census(store)}");
            first.dispose();
            second.dispose();
            print(i"key 2 released: {census(store)}");
            third.dispose();
            print(i"nothing watched: {census(store)}");
            "#,
        ),
        "one read: slots=0 nodes=1 feeds=0\n\
         two on key 2: slots=1 nodes=3 feeds=0\n\
         and key 4's name: slots=2 nodes=6 feeds=0\n\
         key 2 released: slots=1 nodes=5 feeds=0\n\
         nothing watched: slots=0 nodes=1 feeds=0\n",
    );
}

#[test]
fn a149_s3_a_map_field_is_a_map_flow_told_one_op_per_changed_key() {
    // A map field's handle is the Map shape's FLOW: an open flow is told a `Put`
    // per key that arrived or changed and a `Delete` per key that left — through
    // `at`, `insert`, `remove`, a write deep inside one value, a whole write of the
    // map (reconciled by key) or of the root — and nothing for a write that
    // changed nothing. A map operator starts from it: the key set follows one op
    // at a time. The feed goes with the flow.
    assert_compiles_and_runs(
        &collections_program(
            r#"
            let store = Store::new(global());
            let flow = store.channels().open();
            print(i"open: {census(store)}");
            store.channels().insert(7, channel(7, "seven"));
            store.channels().at(1).set(Some(channel(1, "one")));
            let _renamed = store.channels().at(2).some().name().patch("two");
            store.channels().remove(3);
            store.channels().remove(3);
            store.channels().at(4).set(store.channels().at(4).get());
            print(i"handle writes:{map_ops((flow.drain)())}");
            mut whole = store.channels().get();
            whole.remove(4);
            whole.insert(5, channel(5, "five"));
            whole.insert(8, channel(8, "eight"));
            store.channels().set(whole);
            print(i"whole write:{map_ops((flow.drain)())}");
            mut root = store.get();
            root.channels.remove(8);
            store.set(root);
            print(i"root write:{map_ops((flow.drain)())}");
            store.channels().set(store.channels().get());
            print(i"unchanged:{map_ops((flow.drain)())}");
            (flow.release)();
            print(i"released: {census(store)}");
            let keys = store.channels().keys().memo_global();
            store.channels().insert(10, channel(10, "ten"));
            store.channels().remove(1);
            print(i"keys: {keys.len()} {keys.contains(10).get()} {keys.contains(1).get()}");
            "#,
        ),
        "open: slots=0 nodes=2 feeds=1\n\
         handle writes: Put(7, -, seven) Put(1, c1, one) Put(2, c2, two) Delete(3, c3)\n\
         whole write: Delete(4, c4) Put(5, c5, five) Put(8, -, eight)\n\
         root write: Delete(8, eight)\n\
         unchanged: -\n\
         released: slots=0 nodes=1 feeds=0\n\
         keys: 5 true false\n",
    );
}

#[test]
fn a149_s3_a_set_field_wakes_one_member_and_is_told_set_ops() {
    // A set field is a keyed node of flags: `contains(x)` is a `Store<bool>` that
    // wakes when `x` comes or goes and never for another member; `set(true)` and
    // `set(false)` add and remove; an open flow is told `Add`/`Remove`.
    assert_compiles_and_runs(
        &collections_program(
            r#"
            let store = Store::new(global());
            let flow = store.tags().open();
            let owner = Owner::new();
            run_with_owner(owner, || {
                store.tags().contains("red").effect_on_change(|held| bump(i"red:{held}"));
                store.tags().contains("blue").effect_on_change(|held| bump(i"blue:{held}"));
            });
            store.tags().insert("red");
            print(i"red added:{tally()}");
            store.tags().insert("green");
            print(i"green added:{tally()}");
            store.tags().contains("red").set(true);
            print(i"red added again:{tally()}");
            store.tags().remove("red");
            store.tags().contains("blue").set(true);
            print(i"red removed, blue added:{tally()}");
            mut whole: HashSet<str> = HashSet::new();
            whole.insert("blue");
            whole.insert("pink");
            store.tags().set(whole);
            print(i"whole write:{tally()}");
            print(i"ops:{set_ops((flow.drain)())}");
            owner.dispose();
            (flow.release)();
            print(census(store));
            "#,
        ),
        "red added: red:true=1\n\
         green added: -\n\
         red added again: -\n\
         red removed, blue added: blue:true=1 red:false=1\n\
         whole write: -\n\
         ops: Add(red) Add(green) Remove(red) Add(blue) Remove(green) Add(pink)\n\
         slots=0 nodes=1 feeds=0\n",
    );
}

#[test]
fn a149_s3_a_keyed_list_reads_by_key_and_wakes_only_the_element_that_moved() {
    // A list field is a sequence node. `by_key(k)` (Q11: there is no `at(index)`)
    // depends on the element under `k` only: a push of another element, or a
    // whole write that leaves it as it was, never wakes it. `set(None)` removes
    // the element and `set(Some(v))` for a key the list lacks appends.
    assert_compiles_and_runs(
        &collections_program(
            r#"
            let store = Store::new(global());
            store.log().push(Message { id = 1, text = "hi" });
            store.log().push(Message { id = 2, text = "yo" });
            let owner = Owner::new();
            run_with_owner(owner, || {
                store.log().by_key(1).effect_on_change(|_held| bump("m1"));
                store.log().by_key(2).effect_on_change(|_held| bump("m2"));
                store.log().by_key(2).some().text().effect_on_change(|_text| bump("text2"));
                store.log().effect_on_change(|_list| bump("log"));
            });
            store.log().push(Message { id = 3, text = "hey" });
            print(i"another pushed:{tally()}");
            let _edited = store.log().by_key(1).some().text().patch("HI");
            print(i"1 edited by key:{tally()}");
            store.log().set([Message { id = 1, text = "HI" }, Message { id = 2, text = "YO" }, Message { id = 3, text = "hey" }]);
            print(i"whole list, 2 changed:{tally()}");
            store.log().remove_at(0);
            print(i"1 spliced out:{tally()}");
            store.log().splice(0, 1, [Message { id = 2, text = "YO" }]);
            print(i"the same element spliced back:{tally()}");
            store.log().by_key(4).set(Some(Message { id = 4, text = "four" }));
            store.log().by_key(3).set(None);
            print(i"4 appended, 3 removed by key:{tally()} {texts(store.log().get())}");
            owner.dispose();
            print(census(store));
            "#,
        ),
        "another pushed: log=1\n\
         1 edited by key: log=1 m1=1\n\
         whole list, 2 changed: log=1 m2=1 text2=1\n\
         1 spliced out: log=1 m1=1\n\
         the same element spliced back: -\n\
         4 appended, 3 removed by key: log=2 [2:YO 4:four]\n\
         slots=0 nodes=1 feeds=0\n",
    );
}

#[test]
fn a149_s3_a_list_field_is_a_collection_flow_told_splices() {
    // A list field's handle is the sequence shape's FLOW: a push is one
    // `Splice`, an edit by key one `SetAt`, a removal by key a `Splice` out, and a
    // whole write `reconcile_to`'s ONE span between the common prefix and suffix.
    // An operator over it runs its closure once per element that ARRIVED — a
    // push maps one element, not the list.
    assert_compiles_and_runs(
        &collections_program(
            r#"
            let store = Store::new(global());
            let flow = store.log().open();
            store.log().push(Message { id = 1, text = "hi" });
            store.log().push(Message { id = 2, text = "yo" });
            let _edited = store.log().by_key(1).some().text().patch("HI");
            store.log().by_key(2).set(None);
            print(i"writes:{seq_ops((flow.drain)())}");
            store.log().set([Message { id = 0, text = "zero" }, Message { id = 1, text = "HI" }]);
            print(i"whole write:{seq_ops((flow.drain)())}");
            store.log().set(store.log().get());
            print(i"unchanged:{seq_ops((flow.drain)())}");
            (flow.release)();
            let mapped = Shared::new(0);
            let lengths = store.log().map(|message: Message| {
                mapped.write() = mapped.read() + 1;
                message.text.len()
            }).memo_global();
            print(i"sealed: mapped={mapped.read()} {lengths.get().len()}");
            store.log().push(Message { id = 6, text = "six" });
            print(i"one pushed: mapped={mapped.read()} {lengths.get().len()}");
            "#,
        ),
        "writes: Splice(0, [], [1:hi]) Splice(1, [], [2:yo]) SetAt(0, hi, HI) Splice(1, [2:yo], [])\n\
         whole write: Splice(0, [], [0:zero])\n\
         unchanged: -\n\
         sealed: mapped=2 2\n\
         one pushed: mapped=3 3\n",
    );
}

#[test]
fn a149_s3_a_list_field_offers_no_index_handle() {
    // Q11: `at(index)` is not offered on a list field — a position shifts under a
    // splice, and the handle would silently change which element it names.
    assert_fails_with(
        &collections_program(
            r#"
            let store = Store::new(global());
            let _first = store.log().at(0);
            "#,
        ),
        "has no method 'at'",
    );
}

#[test]
fn a149_s3_a_collection_through_an_option_reads_and_patches_by_key() {
    // A collection reached THROUGH a variant or an `Option` keeps its keyed
    // steps: `at`, `contains` and `by_key` are `StoreSome`s that read `None` and
    // patch nothing while the option is `None`, land once it is `Some`, and wake
    // a reader of one key for that key only.
    assert_compiles_and_runs(
        &collections_program(
            r#"
            let store = Store::new(Holder { index = None, members = None, log = None });
            print(store.index().some().at(1).patch(Some("one")));
            store.index().set(Some(HashMap::new()));
            store.members().set(Some(HashSet::new()));
            store.log().set(Some([]));
            let owner = Owner::new();
            run_with_owner(owner, || {
                store.index().some().at(1).effect_on_change(|_held| bump("i1"));
                store.members().some().contains("x").effect_on_change(|_held| bump("x"));
                store.log().some().by_key(4).effect_on_change(|_held| bump("m4"));
            });
            print(store.index().some().at(1).patch(Some("one")));
            print(store.index().some().at(2).patch(Some("two")));
            print(i"index:{tally()} {store.index().some().at(1).some().get().unwrap_or("-")}");
            print(store.members().some().contains("x").patch(true));
            print(store.members().some().contains("y").patch(true));
            print(i"members:{tally()}");
            store.log().some().push(Message { id = 3, text = "three" });
            store.log().some().push(Message { id = 4, text = "four" });
            print(i"log:{tally()} {store.log().some().size()}");
            owner.dispose();
            "#,
        )
        .replace(
            "fun main() {",
            r#"[derive(Storable)]
struct Holder {
    index: Option<HashMap<i32, str>>,
    members: Option<HashSet<str>>,
    log: Option<List<Message>>,
}

fun main() {"#,
        ),
        "false\ntrue\ntrue\nindex: i1=1 one\ntrue\ntrue\nmembers: x=1\nlog: m4=1 2\n",
    );
}
