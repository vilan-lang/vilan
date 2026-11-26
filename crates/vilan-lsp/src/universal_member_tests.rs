//! E371: a member of a UNIVERSAL trait — one implemented for every type
//! through a blanket over a bare binder, the bottom tier of std's
//! impl-specialization pattern (`impl type T with MaybeSignal<T>`) — is a
//! member of every value, so it opened every popup: under the web prelude's
//! imports `.bind`, `.same_element`, `.start_element`,
//! `.start_element_value`, `.store_diff` and `.store_leaf_diff` were offered on
//! an `i32`, a `str` and a user struct alike. Such a member is now admitted only
//! to a typed prefix (E213's three-character rule, B620's `admission`), and
//! where several applying blankets declare one name the candidate is the most
//! specific one's, the impl a call dispatches to.
//!
//! The universality is `vilan_core::impl_select::is_universal_trait`'s; these
//! pins read the completion list the server would filter and render.

use std::path::Path;

use vilan_ide::completion::Completion;

use crate::document::Document;
use crate::document::tests::std_root;

/// The imports an application under `prelude = "std::web::prelude"` has
/// ambient (`vilan/std/src/web/prelude.vl`'s reactive names), plus the store,
/// so `store_core.vl` is loaded and its two universal traits are in the
/// program.
const IMPORTS: &str = "import std::compare::PartialEq;\n\
     import std::display::Display;\n\
     import std::reactive::{ CollPipe, SetPipe, Signal, SignalCell };\n\
     import std::reactive::store::{ Storable, Store };\n\n";

/// The six names the find listed, every one a universal trait's member.
const UNIVERSAL: [&str; 6] = [
    "bind",
    "same_element",
    "start_element",
    "start_element_value",
    "store_diff",
    "store_leaf_diff",
];

/// The candidates offered at `receiver.typed|`, where `template` is a whole
/// program whose `$` stands for the receiver expression. With `$` replaced by
/// the bare receiver the program must check clean (the premise).
fn offered_at(template: &str, receiver: &str, typed: &str) -> Vec<Completion> {
    let premise = format!("{IMPORTS}{}", template.replace('$', receiver));
    let checked = Document::analyze(&premise, &std_root(), Path::new("test.vl"));
    assert!(
        checked.diagnostics.is_empty(),
        "the fixture must check clean: {:?}\n{premise}",
        checked
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect::<Vec<_>>(),
    );
    let text = format!(
        "{IMPORTS}{}",
        template.replace('$', &format!("{receiver}.{typed}"))
    );
    let offset = text
        .find(&format!("{receiver}.{typed};"))
        .expect("the probe is in the text")
        + receiver.len()
        + 1
        + typed.len();
    let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
    document.completion(offset)
}

fn labels(candidates: &[Completion]) -> Vec<&str> {
    candidates
        .iter()
        .map(|candidate| candidate.label.as_str())
        .collect()
}

fn find<'c>(candidates: &'c [Completion], name: &str) -> Option<&'c Completion> {
    candidates.iter().find(|candidate| candidate.label == name)
}

/// The universal member `name`, typed in full but for the last letters, is
/// offered — carrying the admission reason naming `trait_name`.
fn assert_admitted_by_prefix(template: &str, receiver: &str, name: &str, trait_name: &str) {
    let typed = &name[..3];
    let found = offered_at(template, receiver, typed);
    let candidate = find(&found, name).unwrap_or_else(|| {
        panic!(
            "`{receiver}.{typed}` must offer `{name}`: {:?}",
            labels(&found)
        )
    });
    let reason = candidate
        .admission
        .as_deref()
        .unwrap_or_else(|| panic!("`{name}` carries an admission reason"));
    assert_eq!(
        reason,
        format!("every type has this: `{trait_name}` is implemented for all types"),
    );
}

/// None of the six with nothing typed, nor with a two-character prefix, and
/// each admitted to its own three-character prefix.
fn assert_universal_members_sunk(template: &str, receiver: &str) {
    let bare = offered_at(template, receiver, "");
    for name in UNIVERSAL {
        assert!(
            find(&bare, name).is_none(),
            "`{receiver}.` must not offer `{name}` with nothing typed: {:?}",
            labels(&bare),
        );
        let two = &name[..2];
        let short = offered_at(template, receiver, two);
        assert!(
            find(&short, name).is_none(),
            "`{receiver}.{two}` must not offer `{name}`: {:?}",
            labels(&short),
        );
    }
    for (name, trait_name) in [
        ("bind", "MaybeSignal"),
        ("same_element", "SameElement"),
        ("start_element", "IntoFlow"),
        ("start_element_value", "IntoElement"),
        ("store_diff", "Storable"),
        ("store_leaf_diff", "StoreLeaf"),
    ] {
        assert_admitted_by_prefix(template, receiver, name, trait_name);
    }
}

/// The find's own receiver, an `i32`.
#[test]
fn an_integer_offers_no_universal_member_until_its_prefix_is_typed() {
    let template = "fun main() {\n\tlet n = 1;\n\tlet _ = $;\n}\n";
    assert_universal_members_sunk(template, "n");
    // Its own members are untouched.
    let bare = offered_at(template, "n", "");
    assert!(
        find(&bare, "to_string").is_some_and(|candidate| candidate.admission.is_none()),
        "{:?}",
        labels(&bare)
    );
}

#[test]
fn a_string_offers_no_universal_member_until_its_prefix_is_typed() {
    assert_universal_members_sunk(
        "fun main() {\n\tlet text = \"hi\";\n\tlet _ = $;\n}\n",
        "text",
    );
}

#[test]
fn a_user_struct_offers_no_universal_member_until_its_prefix_is_typed() {
    let template = "struct Point {\n\tx: i32,\n}\n\n\
         fun main() {\n\tlet point = Point { x = 1 };\n\tlet _ = $;\n}\n";
    assert_universal_members_sunk(template, "point");
    let bare = offered_at(template, "point", "");
    assert!(
        find(&bare, "x").is_some_and(|candidate| candidate.admission.is_none()),
        "the field stays: {:?}",
        labels(&bare)
    );
}

/// The derive's NOMINAL `impl User with Storable` offers `store_diff` through
/// the member table, not the blanket — and is admitted the same way.
#[test]
fn a_derived_storable_struct_offers_store_diff_only_to_its_prefix() {
    let template = "[derive(Storable)]\nstruct User {\n\tname: str,\n}\n\n\
         fun main() {\n\tlet user = User { name = \"a\" };\n\tlet _ = $;\n}\n";
    let bare = offered_at(template, "user", "");
    assert!(find(&bare, "store_diff").is_none(), "{:?}", labels(&bare));
    assert!(find(&bare, "name").is_some(), "{:?}", labels(&bare));
    let typed = offered_at(template, "user", "store_d");
    let candidate = find(&typed, "store_diff").expect("`user.store_d` offers `store_diff`");
    assert!(candidate.admission.is_some());
    // The nominal member, not the blanket's: the derive's impl names `User`.
    let detail = candidate.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("impl User with Storable"),
        "the derive's member: {detail}"
    );
}

/// The flow surface (an INHERENT blanket's `derive`, the cell's own `set` and
/// `get`, `Flow`'s `sub`) is untouched; only `bind` sinks. With `bin` typed the
/// candidate is the `F: Flow` impl's member — the one a call dispatches to —
/// which its `detail` says (the signature label carries the impl header).
#[test]
fn a_signal_cell_keeps_its_flow_surface_and_binds_through_the_flow_tier() {
    let template = "fun pick(cell: SignalCell<i32>) {\n\tlet _ = $;\n}\n";
    let bare = offered_at(template, "cell", "");
    for name in ["derive", "sub", "set", "get"] {
        let candidate = find(&bare, name)
            .unwrap_or_else(|| panic!("`cell.` offers `{name}`: {:?}", labels(&bare)));
        assert!(
            candidate.admission.is_none(),
            "`{name}` is no universal member"
        );
    }
    assert!(find(&bare, "bind").is_none(), "{:?}", labels(&bare));
    let typed = offered_at(template, "cell", "bin");
    let bind = find(&typed, "bind").expect("`cell.bin` offers `bind`");
    assert!(bind.admission.is_some());
    let detail = bind.detail.as_deref().unwrap_or_default();
    assert!(
        detail.starts_with("impl type F: Flow<type T> with MaybeSignal<T>\n"),
        "the most specific tier's member: {detail}"
    );
}

/// The unbounded blanket's member where it is the only tier that applies.
#[test]
fn an_integer_binds_through_the_unbounded_tier() {
    let found = offered_at("fun main() {\n\tlet n = 1;\n\tlet _ = $;\n}\n", "n", "bin");
    let detail = find(&found, "bind")
        .and_then(|bind| bind.detail.as_deref())
        .unwrap_or_default();
    assert!(
        detail.starts_with("impl type T with MaybeSignal<T>\n"),
        "{detail}"
    );
}

/// A user's own tiered trait: `Differs` is universal (an unbounded tier),
/// `Second` is universal through it, and `Shout` is not (its one impl is
/// bounded by `Display`, which not every type is).
const USER_TIERS: &str = "trait Differs {\n\tfun differs_from(&self, other: &Self): bool;\n}\n\n\
     impl type T with Differs {\n\tfun differs_from(&self, other: &T): bool {\n\t\ttrue\n\t}\n}\n\n\
     impl type T: PartialEq with Differs {\n\tfun differs_from(&self, other: &T): bool {\n\t\t*self != *other\n\t}\n}\n\n\
     trait Second {\n\tfun second_opinion(&self): bool;\n}\n\n\
     impl type T: Differs with Second {\n\tfun second_opinion(&self): bool {\n\t\tself.differs_from(self)\n\t}\n}\n\n\
     trait Shout {\n\tfun shout(self): str;\n}\n\n\
     impl type T: Display with Shout {\n\tfun shout(self): str {\n\t\tself.to_string()\n\t}\n}\n\n\
     struct Point {\n\tx: i32,\n}\n\n";

#[test]
fn a_user_universal_tier_trait_is_sunk_on_an_integer_and_a_struct() {
    for (setup, receiver) in [
        ("let n = 1;", "n"),
        ("let point = Point { x = 1 };", "point"),
    ] {
        let template = format!("{USER_TIERS}fun main() {{\n\t{setup}\n\tlet _ = $;\n}}\n");
        let bare = offered_at(&template, receiver, "");
        assert!(
            find(&bare, "differs_from").is_none(),
            "`{receiver}.`: {:?}",
            labels(&bare)
        );
        assert_admitted_by_prefix(&template, receiver, "differs_from", "Differs");
    }
}

/// Universal TRANSITIVELY: `impl type T: Differs with Second`'s one bound is
/// itself a universal trait.
#[test]
fn a_trait_universal_through_its_bound_is_sunk() {
    let template = format!("{USER_TIERS}fun main() {{\n\tlet n = 1;\n\tlet _ = $;\n}}\n");
    let bare = offered_at(&template, "n", "");
    assert!(
        find(&bare, "second_opinion").is_none(),
        "{:?}",
        labels(&bare)
    );
    assert_admitted_by_prefix(&template, "n", "second_opinion", "Second");
}

/// A blanket bounded by a trait not every type has is no universal tier:
/// offered with nothing typed, no admission.
#[test]
fn a_bounded_blanket_trait_is_offered_normally() {
    let template = format!("{USER_TIERS}fun main() {{\n\tlet n = 1;\n\tlet _ = $;\n}}\n");
    let bare = offered_at(&template, "n", "");
    let shout = find(&bare, "shout").unwrap_or_else(|| panic!("{:?}", labels(&bare)));
    assert!(shout.admission.is_none());
}

/// Exception (b) as it stands: a receiver typed by a parameter whose BOUND
/// names the trait. Member completion offers nothing on a bare type
/// parameter today — the blanket selection asks for a concrete type
/// (`impl_select::is_resolvable`) and no nominal names one — so `bind` is not
/// offered there with or without a prefix, and E371 sinks nothing there either.
/// When bound-directed members are offered, this pin is the place to say
/// `bind` comes WITHOUT an admission: there the bound is the point.
#[test]
fn a_bound_typed_receiver_is_not_sunk() {
    let template = "fun pick<M: MaybeSignal<i32>>(value: M) {\n\tlet _ = $;\n}\n";
    let template = format!("import std::reactive::MaybeSignal;\n{template}");
    for typed in ["", "bin"] {
        let found = offered_at(&template, "value", typed);
        assert!(
            found.iter().all(|candidate| candidate.admission.is_none()),
            "`value.{typed}`: {:?}",
            labels(&found)
        );
    }
}
