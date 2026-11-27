//! B711 S2a (`struct-keys.md` §3.5, R6): field syntax through a
//! `[field_syntax]` method in the editor. The analysis reads `h.address.city`
//! as `h.at(User.address).at(Address.city)`; the editor reads the same reader
//! (`vilan_core::field_syntax`), so completion offers the fields the analysis
//! reads through, hover and go to definition answer for the FIELD, and a
//! field's references and rename reach its field-syntax reads — std's store
//! handles' reads (`app.user`, through their own door since S2b) included.

use std::path::Path;

use super::Document;
use super::tests::{on_big_stack, std_root};

const SOURCE: &str = "import std::shared::Shared;\n\n\
     struct Address {\n\t/// The town.\n\tcity: str,\n\tzip: i32,\n}\n\n\
     struct User {\n\tname: str,\n\taddress: Address,\n\tpeek: str,\n}\n\n\
     struct Cursor<S, T> {\n\troot: Shared<S>,\n\tkey: Key<S, T>,\n}\n\n\
     impl Cursor<type S, type T> {\n\
     \tfun peek(self): T {\n\t\tself.root.read()[self.key]\n\t}\n\n\
     \t[field_syntax]\n\tfun at<U>(self, key: Key<T, U>): Cursor<S, U> {\n\
     \t\tCursor { root = self.root, key = self.key.then(key) }\n\t}\n}\n\n\
     fun main() {\n\
     \tlet user = User { name = \"ada\", address = Address { city = \"x\", zip = 1 }, peek = \"p\" };\n\
     \tlet h = Cursor { root = Shared::new(user), key = User.address };\n\
     \tlet city = h.city;\n\
     \tprint(city.peek());\n\
     \tlet u = Cursor { root = Shared::new(user), key = User.address.city };\n\
     \tprint(u.peek());\n\
     }\n";

/// A root handle over `User` itself, so `r.address.city` is a chain of two
/// reads: `Cursor` holds a key, and S1 has no identity key to start one at the
/// root (`Key<S, S>`), so the root is its own type.
const CHAIN: &str = "import std::shared::Shared;\n\n\
     struct Address {\n\tcity: str,\n}\n\n\
     struct User {\n\taddress: Address,\n}\n\n\
     struct Cursor<S, T> {\n\troot: Shared<S>,\n\tkey: Key<S, T>,\n}\n\n\
     impl Cursor<type S, type T> {\n\
     \t[field_syntax]\n\tfun at<U>(self, key: Key<T, U>): Cursor<S, U> {\n\
     \t\tCursor { root = self.root, key = self.key.then(key) }\n\t}\n\n\
     \tfun peek(self): T {\n\t\tself.root.read()[self.key]\n\t}\n}\n\n\
     struct Root<S> {\n\tstate: Shared<S>,\n}\n\n\
     impl Root<type S> {\n\
     \t[field_syntax]\n\tfun at<U>(self, key: Key<S, U>): Cursor<S, U> {\n\
     \t\tCursor { root = self.state, key }\n\t}\n}\n\n\
     fun main() {\n\
     \tlet r = Root { state = Shared::new(User { address = Address { city = \"x\" } }) };\n\
     \tprint(r.address.city.peek());\n\
     }\n";

fn analyzed(text: &str) -> Document {
    Document::analyze(text, &std_root(), Path::new("test.vl"))
}

fn messages(document: &Document) -> Vec<String> {
    document
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.msg.clone())
        .collect()
}

/// `text` with the rename `at -> to` applied, as the editor applies it.
fn renamed(document: &Document, text: &str, at: usize, to: &str) -> String {
    let edits = document
        .rename_edits(at, to)
        .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
    let mut spans: Vec<(usize, usize)> = edits
        .iter()
        .map(|(_, span, _)| (span.start, span.end))
        .collect();
    spans.sort_unstable();
    let mut renamed = text.to_string();
    for (start, end) in spans.into_iter().rev() {
        renamed.replace_range(start..end, to);
    }
    renamed
}

#[test]
fn field_syntax_files_analyze_clean() {
    on_big_stack(|| {
        for text in [SOURCE, CHAIN] {
            let document = analyzed(text);
            assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
        }
    });
}

/// After `h.`: the handle's own members first (its fields `root` and `key`,
/// its methods), then the fields of `Address` it reads through, each with the
/// read's RESULT type. `peek` is not offered as a field: the handle's own
/// `peek()` shadows it.
#[test]
fn field_syntax_completion_offers_the_keys_fields_after_the_handles_members() {
    on_big_stack(|| {
        let text = SOURCE.replace("let city = h.city;", "let city = h.;");
        let at = text.find("h.;").expect("the member") + "h.".len();
        let document = analyzed(&text);
        let completed = document.completion(at);
        let rows: Vec<(String, Option<String>)> = completed
            .iter()
            .map(|candidate| (candidate.label.clone(), candidate.detail.clone()))
            .collect();
        let position = |label: &str| rows.iter().position(|(row, _)| row == label);
        let city = position("city").unwrap_or_else(|| panic!("`city` offered: {rows:?}"));
        assert_eq!(
            rows[city].1.as_deref(),
            Some("Cursor<User, str>"),
            "{rows:?}"
        );
        let zip = position("zip").expect("`zip` offered");
        assert_eq!(rows[zip].1.as_deref(), Some("Cursor<User, i32>"));
        for own in ["root", "key", "peek", "at"] {
            let own = position(own).unwrap_or_else(|| panic!("`{own}` offered: {rows:?}"));
            assert!(own < city, "the handle's members come first: {rows:?}");
        }
        assert_eq!(
            rows.iter().filter(|(label, _)| label == "peek").count(),
            1,
            "{rows:?}"
        );
        let city = &completed[city];
        assert_eq!(city.documentation.as_deref(), Some("The town."));
    });
}

/// `r.address.` mid-edit: the receiver is itself a field-syntax read, typed
/// through the method's result (`Cursor<User, Address>`), which offers
/// `Address`'s fields.
#[test]
fn field_syntax_a_chains_receiver_types_through_the_read() {
    on_big_stack(|| {
        let text = CHAIN.replace("r.address.city.peek()", "r.address.");
        let at = text.find("r.address.").expect("the chain") + "r.address.".len();
        let document = analyzed(&text);
        let completed = document.completion(at);
        let city = completed
            .iter()
            .find(|candidate| candidate.label == "city")
            .unwrap_or_else(|| {
                panic!(
                    "`city` offered: {:?}",
                    completed.iter().map(|c| &c.label).collect::<Vec<_>>()
                )
            });
        assert_eq!(city.detail.as_deref(), Some("Cursor<User, str>"));
    });
}

/// Hover on the member: the read's type, the call it is, then the field's
/// declaration and its `///`; go to definition lands on the field.
#[test]
fn field_syntax_hover_and_definition_answer_for_the_field() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let member = SOURCE.find("h.city;").expect("the read") + "h.c".len();
        let hover = document.hover(member).expect("a hover on the member");
        assert!(hover.contains("city: Cursor<User, str>"), "{hover}");
        assert!(hover.contains(".at(Address.city)"), "{hover}");
        assert!(hover.contains("struct Address"), "{hover}");
        assert!(hover.contains("The town."), "{hover}");
        let (_, field) = document.definition(member).expect("the field");
        assert_eq!(field.start, SOURCE.find("city: str").expect("the field"));
    });
}

/// A field's references list its field-syntax reads, and a rename rewrites
/// them — a chain's middle member and its last — and the file checks clean.
#[test]
fn field_syntax_renaming_a_field_rewrites_its_reads() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let declaration = SOURCE.find("city: str").expect("the field");
        let read = SOURCE.find("h.city;").expect("the read") + "h.".len();
        let starts: Vec<usize> = document
            .references(declaration)
            .into_iter()
            .map(|(_, span)| span.start)
            .collect();
        assert!(starts.contains(&read), "{starts:?}");
        let after = renamed(&document, SOURCE, declaration, "town");
        assert!(after.contains("h.town;"), "{after}");
        assert!(after.contains("User.address.town"), "{after}");
        assert!(after.contains("Address { town = "), "{after}");
        let checked = analyzed(&after);
        assert!(checked.diagnostics.is_empty(), "{:?}", messages(&checked));
        // The middle of a chain.
        let document = analyzed(CHAIN);
        let middle = CHAIN.find("address: Address").expect("the field");
        let after = renamed(&document, CHAIN, middle, "home");
        assert!(after.contains("r.home.city.peek()"), "{after}");
        let checked = analyzed(&after);
        assert!(checked.diagnostics.is_empty(), "{:?}", messages(&checked));
        // And its last member.
        let last = CHAIN.find("city: str").expect("the field");
        let after = renamed(&document, CHAIN, last, "town");
        assert!(after.contains("r.address.town.peek()"), "{after}");
        let checked = analyzed(&after);
        assert!(checked.diagnostics.is_empty(), "{:?}", messages(&checked));
    });
}

/// A149 S4's store reads are field reads too: a `Storable` struct's field
/// lists `store.address` among its references (before S2a the read was only
/// its projection method's). Renaming such a field stays refused, by the rule
/// that guards every field `[derive(..)]` code reads — the generated
/// projections have no file to edit.
#[test]
fn field_syntax_a_storable_fields_references_list_its_store_reads() {
    on_big_stack(|| {
        let text = "import std::reactive::{ Signal, Source };\n\
             import std::reactive::store::{ Storable, Store };\n\n\
             [derive(Storable)]\nstruct Address {\n\tcity: str,\n}\n\n\
             [derive(Storable)]\nstruct App {\n\tname: str,\n\taddress: Address,\n}\n\n\
             fun main() {\n\tlet store = Store::new(App { name = \"a\", address = Address { city = \"c\" } });\n\
             \tprint(store.address.city.get());\n}\n";
        let document = analyzed(text);
        assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
        let middle = text.find("address: Address").expect("the field");
        let read = text.find("store.address.").expect("the read") + "store.".len();
        let starts: Vec<usize> = document
            .references(middle)
            .into_iter()
            .map(|(_, span)| span.start)
            .collect();
        assert!(starts.contains(&read), "{starts:?}");
        let last = text.find("city: str").expect("the field");
        let read = text.find(".city.get()").expect("the read") + 1;
        let starts: Vec<usize> = document
            .references(last)
            .into_iter()
            .map(|(_, span)| span.start)
            .collect();
        assert!(starts.contains(&read), "{starts:?}");
        let refusal = document
            .rename_edits(middle, "home")
            .expect_err("a derived struct's field is not renamed");
        assert!(
            refusal.message().contains("[derive(..)]"),
            "{}",
            refusal.message()
        );
    });
}

/// B711 S2b: a store read is the door's call like any other: hover shows the
/// handle type and `.field(Address.city)`, definition lands on the FIELD, and
/// completion after a store receiver offers the struct's fields with the
/// handle type as detail.
#[test]
fn field_syntax_a_store_read_hovers_defines_and_completes_through_the_door() {
    on_big_stack(|| {
        let text = "import std::reactive::{ Signal, Source };\n\
             import std::reactive::store::{ Storable, Store };\n\n\
             [derive(Storable)]\nstruct Address {\n\t/// The town.\n\tcity: str,\n}\n\n\
             [derive(Storable)]\nstruct App {\n\tname: str,\n\taddress: Address,\n}\n\n\
             fun main() {\n\tlet store = Store::new(App { name = \"a\", address = Address { city = \"c\" } });\n\
             \tprint(store.address.city.get());\n}\n";
        let document = analyzed(text);
        assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
        let member = text.find(".city.get()").expect("the read") + ".ci".len();
        let hover = document.hover(member).expect("a hover on the member");
        assert!(hover.contains("Store<str>"), "{hover}");
        assert!(hover.contains(".field(Address.city)"), "{hover}");
        assert!(hover.contains("The town."), "{hover}");
        let (_, field) = document.definition(member).expect("the field");
        assert_eq!(field.start, text.find("city: str").expect("the field"));
    });
}

/// What a field-syntax member colours as: the existing classification reads
/// the member span of a call, so it is a method's colour, not a field's (not
/// changed in S2a; recorded here so a change is a decision).
#[test]
fn field_syntax_a_members_colour_is_the_calls() {
    on_big_stack(|| {
        let document = analyzed(SOURCE);
        let tokens = document.semantic_tokens();
        let member = SOURCE.find("h.city;").expect("the read") + "h.".len();
        let kind = tokens
            .iter()
            .find(|(span, _, _)| span.start == member)
            .map(|(_, kind, _)| format!("{kind:?}"));
        assert_eq!(kind.as_deref(), Some("Method"));
    });
}
