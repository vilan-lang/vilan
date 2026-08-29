//! The parse-error blackout, blanket-vs-specific dispatch (B73/B127/B130),
//! remote sources, `std::markdown`, `std::path`, and the const asset
//! channel's file half — `asset::bundle`, and 035's `bundle_as` / `read_dir` /
//! `read_dir_all` / `digest`.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- The blackout: analysis survives a parse error (editing-dx.md S1/S6) -------
//
// `analyze_source` runs the analyzer on the SALVAGED tree, so these pins measure
// what the language server publishes and — since S6 — what batch `vilan check`
// prints. The survey's §2 finding is that a file which does not parse used to
// lose every diagnostic it already had: `vilan check` analyzed nothing at all
// (mechanism 1, P29), and the salvage the server did get was prefix-only, with
// the enclosing body emptied (mechanisms 2 and 3, P30/P31).
//
// Each pin below asserts BOTH halves — the parse error is reported, AND the
// analyzer diagnostics elsewhere in the file are still there — because either one
// alone passes vacuously on the behavior it is meant to exclude.

/// Every diagnostic message a source produces, parse and analysis together, in
/// the order `analyze_source` returns them.
#[track_caller]
fn all_diagnostics(source: &str) -> Vec<String> {
    match compile(source) {
        Ok(_) => panic!("expected diagnostics, but it compiled cleanly"),
        Err(messages) => messages,
    }
}

#[track_caller]
fn assert_reports_all(source: &str, expected: &[&str]) {
    let messages = all_diagnostics(source);
    for part in expected {
        assert!(
            messages.iter().any(|message| message.contains(part)),
            "no diagnostic contains {part:?}; got: {messages:#?}"
        );
    }
}

/// P29, the batch shape: one parse error in `broken`, two genuine type errors in
/// `main`. All three are reported. Before S1/S6 this file produced exactly one
/// diagnostic — the parse error — and one missing `;` anywhere in a file blinded
/// the checker to everything else in it.
#[test]
fn a_parse_error_does_not_hide_the_analyzer_errors_in_another_function() {
    assert_reports_all(
        "import std::print;\n\
         fun broken() {\n\
         \tlet a: i32 = 1\n\
         \tprint(a);\n\
         }\n\
         fun main() {\n\
         \tlet bad: i32 = \"text\";\n\
         \tlet other: str = 5;\n\
         \tprint(bad);\n\
         }\n",
        &[
            "expected `;` to end this statement",
            "Expected i32, but got str instead.",
            "Expected str, but got i32 instead.",
        ],
    );
}

/// P30 stage 2, the keystroke the survey measured: a standing type error one line
/// above a half-typed call. The editor used to show ONLY `found '}' expected an
/// expression`, anchored on a brace the user never touched — the standing error
/// disappeared for the two keystrokes between `print(` and `print(1)`.
#[test]
fn a_standing_type_error_survives_a_half_typed_call_below_it() {
    assert_reports_all(
        "fun main() {\n\tlet wrong: i32 = \"text\";\n\tprint(\n}\n",
        &[
            "unclosed `(`: expected a matching `)`",
            "Expected i32, but got str instead.",
        ],
    );
}

/// P31 row B, the shape a typing user is in constantly: an unclosed `(` ABOVE a
/// type error. The row the survey singles out as losing the most — "everything
/// below the cursor stops being checked" — because an unclosed region defeated
/// recovery and the statement loop stopped there.
#[test]
fn an_unclosed_delimiter_above_a_type_error_no_longer_erases_it() {
    assert_reports_all(
        "fun one() {\n\
         \tprint(\n\
         }\n\
         fun two() {\n\
         \tlet bad: i32 = \"text\";\n\
         }\n",
        &[
            "unclosed `(`: expected a matching `)`",
            "Expected i32, but got str instead.",
        ],
    );
}

/// The same, in ONE body: the broken statement is dropped and its siblings are
/// still analyzed. This is mechanism 2 — the enclosing `{…}` used to be skipped
/// wholesale and replaced by an empty block, so every statement in the body
/// stopped existing along with the half-typed one.
#[test]
fn a_broken_statement_does_not_erase_its_siblings_diagnostics() {
    assert_reports_all(
        "fun main() {\n\
         \tlet above: i32 = \"text\";\n\
         \tlet broken: i32 = ;\n\
         \tlet below: str = 5;\n\
         }\n",
        &[
            "found ';' expected an expression",
            "Expected i32, but got str instead.",
            "Expected str, but got i32 instead.",
        ],
    );
}

/// The analyzer says nothing INSIDE a recovered region — §13.1's proposed
/// mitigation ("suppress diagnostics whose span falls inside one") turns out to
/// need no code, because a salvaged tree holds nothing there to diagnose: the
/// garbled `(1 +)` becomes a `Node::Error` placeholder that types as nothing and
/// reports nothing, while the standing error beside it is untouched.
#[test]
fn a_recovered_region_produces_no_analyzer_diagnostics_of_its_own() {
    let messages =
        all_diagnostics("fun main() {\n\tlet x: i32 = (1 +);\n\tlet bad: i32 = \"text\";\n}\n");
    assert_eq!(
        messages.len(),
        2,
        "the parse error and the standing error, and nothing from the placeholder: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("found ')' expected an expression"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Expected i32, but got str instead."))
    );
}

/// A parse error must not MANUFACTURE a diagnostic either — the other direction
/// of §8 clause 3, and the one a synchronizer gets wrong by default. A statement
/// whose only fault is its missing `;` is kept, so the names it binds stay bound
/// and the lines below it, which are correct, stay quiet.
#[test]
fn a_missing_semicolon_does_not_unbind_what_its_statement_declared() {
    let messages = all_diagnostics(
        "import std::print;\n\
         fun main() {\n\
         \tlet origin: i32 = 3\n\
         \tlet total: i32 = origin + 1;\n\
         \tprint(total);\n\
         }\n",
    );
    assert_eq!(
        messages.len(),
        1,
        "one missing token, one diagnostic — and no `cannot find 'origin'`: {messages:#?}"
    );
    assert!(messages[0].contains("expected `;` to end this statement"));
}

// --- B73: blanket-vs-specific (method-resolution.md §13) ---------------------
//
// RULED 2026-08-18 as recommended and SHIPPED the same day (R1 eafe5a3e,
// R2 4e086a5d, R3 9d72f2e6 — method-resolution.md §13.8). Every pin below is
// LIVE and asserts the shipped semantics — R1 (the trait's effective
// arguments join the resolution key), R2 (the expected type selects among
// argument-distinct homes), R3 (specificity ranks a genuine overlap). The
// residue §13.8 deferred, B128, is closed (2026-08-23) — its pins sit after
// the §14 deletion block below. The `Into` pins originally staged their second
// home with std's `Into<T>` blanket; §14 deleted it, so they stage the same
// shapes with user impls (each rewrite plant-proven red: the R1 home key
// collapsed to the bare trait id reds the direct-call pins, the re-point's
// selection disabled reds the trait-qualified one).

/// §13.2 row 1, closed by R2, then simplified by §14: std's
/// `impl type T with Into<T>` was a candidate for every receiver and, being
/// tier 0, sorted first, so this program reported `Expected Bar, but got Foo
/// instead.` and the user's own impl was dead code — R1 split the homes and R2
/// let the `let`'s annotation say which was meant. The blanket is deleted now,
/// so the user's impl is the only home and the annotated call reaches it with
/// nothing to select against. (R2's let-annotation selection between two live
/// homes stays pinned by the rows-6/7 and rows-18/19 pins.)
#[test]
fn b73_an_annotated_into_call_reaches_the_user_impl() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::into::Into;

        struct Foo { n: i32 }
        struct Bar { n: i32 }

        impl Foo with Into<Bar> {
            fun into(self): Bar { Bar { n = self.n + 100 } }
        }

        fun main() {
            let b: Bar = Foo { n = 1 }.into();
            print(b.n);
        }
        "#,
        "101\n",
    );
}

/// §13.2 row 2 — the beta-critical miscompile, closed by R1. As filed, the
/// second home was std's `Into<T>` blanket: this program with only the
/// `Into<str>` impl compiled clean, exited 0, and printed `[ 1 ]`, because the
/// blanket's identity `into` was emitted and the user's never was. The blanket
/// is deleted (§14), so the pin keeps R1's point with two USER impls — before
/// R1, one home meant `candidates.first()` and the first-declared impl
/// answered silently. With the trait's arguments in the key the two are
/// separate homes (`Into<str>` and `Into<Bar>`), and with no expected type to
/// steer R2's selection the call is reported rather than silently resolved.
#[test]
fn b73_an_unannotated_into_call_is_ambiguous_rather_than_silently_first_declared() {
    assert_fails_with(
        r#"
        import std::print;
        import std::into::Into;
        import std::string::str;

        struct Foo { n: i32 }
        struct Bar { n: i32 }

        impl Foo with Into<str> {
            fun into(self): str { "converted" }
        }

        impl Foo with Into<Bar> {
            fun into(self): Bar { Bar { n = self.n + 100 } }
        }

        fun main() {
            let s = Foo { n = 1 }.into();
            print(s);
        }
        "#,
        "ambiguous",
    );
}

/// §13.2 row 3, closed by R2 — the same defect in RETURN position, where the
/// expectation comes from the declared return type rather than from a `let`.
/// As filed the competing home was std's blanket (before R2: `Expected Bar,
/// but got Foo instead.` on the `x.into()` tail); the blanket is deleted
/// (§14), so a second user impl — declared FIRST, so first-declared cannot
/// masquerade as selection — keeps the two homes this leg selects between.
#[test]
fn b73_an_into_call_in_return_position_reaches_the_user_impl() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::into::Into;
        import std::string::str;

        struct Foo { n: i32 }
        struct Bar { n: i32 }

        impl Foo with Into<str> {
            fun into(self): str { "converted" }
        }

        impl Foo with Into<Bar> {
            fun into(self): Bar { Bar { n = self.n + 100 } }
        }

        fun to_bar(x: Foo): Bar { x.into() }

        fun main() { print(to_bar(Foo { n = 1 }).n); }
        "#,
        "101\n",
    );
}

/// §13.2 row 5, closed by R2. §3.1's disambiguator is no escape hatch here —
/// both candidates have the same trait head, so naming it settles nothing; the
/// annotation is what picks the home. Two defects stood between: the path head
/// `Into::into` never reached §3.1's re-point at all, because std's blanket's
/// GENERIC subject compare_typed the bare trait type and answered the static
/// path itself (a `self`-method can no longer do that, and the blanket is gone
/// — §14); and the re-point's own provider scan took the first impl of the
/// trait rather than the one the expectation names. The second user impl —
/// declared FIRST — is what keeps that provider scan a real selection.
#[test]
fn b73_a_trait_qualified_into_call_reaches_the_user_impl() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::into::Into;
        import std::string::str;

        struct Foo { n: i32 }
        struct Bar { n: i32 }

        impl Foo with Into<str> {
            fun into(self): str { "converted" }
        }

        impl Foo with Into<Bar> {
            fun into(self): Bar { Bar { n = self.n + 100 } }
        }

        fun main() {
            let b: Bar = Into::into(Foo { n = 1 });
            print(b.n);
        }
        "#,
        "101\n",
    );
}

/// §13.2 rows 6–7, the ordering-sensitive form: a user-written blanket, so std's
/// tier-0 position is not what decides it. Before, the blanket-first program
/// printed `1` and the specific-first program printed `101` — the same two
/// impls, two answers, decided by which block was typed above the other.
///
/// Closed by **R2**, not R3 as §13.4(a) predicted: `impl type T with Conv<T>`
/// instantiates as `Conv<Foo>` on a `Foo` while the specific impl is
/// `Conv<Bar>`, so the two are argument-distinct HOMES and the `let`'s
/// annotation selects between them before specificity is ever consulted. Rows
/// 21–22, whose trait takes no arguments, are the shape that genuinely needs
/// ranking.
#[test]
fn b73_a_user_blanket_loses_to_a_specific_impl_whatever_the_order() {
    let blanket_first = r#"
        import std::print;

        trait Conv<T> { fun conv(self): T; }

        struct Foo { n: i32 }
        struct Bar { n: i32 }

        impl type T with Conv<T> { fun conv(self): T { self } }

        impl Foo with Conv<Bar> { fun conv(self): Bar { Bar { n = self.n + 100 } } }

        fun main() {
            let b: Bar = Foo { n = 1 }.conv();
            print(b.n);
        }
        "#;
    let specific_first = r#"
        import std::print;

        trait Conv<T> { fun conv(self): T; }

        struct Foo { n: i32 }
        struct Bar { n: i32 }

        impl Foo with Conv<Bar> { fun conv(self): Bar { Bar { n = self.n + 100 } } }

        impl type T with Conv<T> { fun conv(self): T { self } }

        fun main() {
            let b: Bar = Foo { n = 1 }.conv();
            print(b.n);
        }
        "#;
    assert_compiles_and_runs(blanket_first, "101\n");
    assert_compiles_and_runs(specific_first, "101\n");
}

/// §13.2 rows 9–10, closed by R3. The plainest specificity case and the one
/// §13.4(a)'s subsumption rule exists for: `Box<i32>` is matched by
/// `Box<type T>` but not conversely, so the concrete impl is strictly more
/// specific. Before: `1` when the generic block was first, `2` when the
/// concrete one was.
#[test]
fn b73_a_concrete_impl_subject_outranks_a_generic_one() {
    let generic_first = r#"
        import std::print;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        impl Box<i32> with Tag { fun tag(self): i32 { 2 } }

        fun main() { print(Box { v = 5 }.tag()); }
        "#;
    let concrete_first = r#"
        import std::print;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }

        impl Box<i32> with Tag { fun tag(self): i32 { 2 } }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        fun main() { print(Box { v = 5 }.tag()); }
        "#;
    assert_compiles_and_runs(generic_first, "2\n");
    assert_compiles_and_runs(concrete_first, "2\n");
}

/// §13.2 rows 11–12, closed by R3's second tier. Equal subject shapes ranked by
/// their binders' BOUNDS — the comparison B98 already runs for its sameness key
/// (`trait-objects.md` §15.8, "Bounds, not identity"). Before: `1`
/// unbounded-first, `2` bounded-first.
#[test]
fn b73_a_bounded_impl_subject_outranks_an_unbounded_one() {
    let unbounded_first = r#"
        import std::print;
        import std::display::Display;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        impl Box<type T: Display> with Tag { fun tag(self): i32 { 2 } }

        fun main() { print(Box { v = 5 }.tag()); }
        "#;
    let bounded_first = r#"
        import std::print;
        import std::display::Display;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }

        impl Box<type T: Display> with Tag { fun tag(self): i32 { 2 } }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        fun main() { print(Box { v = 5 }.tag()); }
        "#;
    assert_compiles_and_runs(unbounded_first, "2\n");
    assert_compiles_and_runs(bounded_first, "2\n");
}

/// §13.2 rows 13–14, closed by R3 — the NESTED form: specificity has to see
/// through a type argument's own arguments, not just the outermost head.
/// Before: `1` generic-first, `2` concrete-first.
#[test]
fn b73_a_nested_concrete_impl_subject_outranks_a_generic_one() {
    let generic_first = r#"
        import std::print;
        import std::list::List;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        impl Box<List<i32>> with Tag { fun tag(self): i32 { 2 } }

        fun main() { print(Box { v = [1, 2, 3] }.tag()); }
        "#;
    let concrete_first = r#"
        import std::print;
        import std::list::List;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }

        impl Box<List<i32>> with Tag { fun tag(self): i32 { 2 } }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        fun main() { print(Box { v = [1, 2, 3] }.tag()); }
        "#;
    assert_compiles_and_runs(generic_first, "2\n");
    assert_compiles_and_runs(concrete_first, "2\n");
}

/// §13.2 row 15 — the same defect as a FALSE REJECTION, and §13.6 Q6's subject,
/// ruled in scope and closed by R3's applicability step. Candidate selection
/// ignored whether an impl's bounds hold, so the `Display`-bounded block won the
/// race on `Box<Opaque>` and then failed its own bound check while the unbounded
/// block that does apply sat below it: `'Opaque' does not implement trait
/// 'Display', required by a generic bound of this call`.
#[test]
fn b73_an_applicable_unbounded_impl_survives_an_inapplicable_bounded_one() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }
        struct Opaque { z: i32 }

        impl Box<type T: Display> with Tag { fun tag(self): i32 { 2 } }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        fun main() { print(Box { v = Opaque { z = 1 } }.tag()); }
        "#,
        "1\n",
    );
}

/// §13.2 row 17, and §13.6 Q5 (ruled `9`) — the least obvious row in the table,
/// closed by R3. A blanket that DECLARES the name beat a more specific impl
/// taking the trait's default, because only a declaration contributed a
/// candidate: `1` in BOTH orders, so the default `9` was unreachable for `Foo`
/// no matter how the file was arranged. An impl that inherits its trait's
/// default now contributes a candidate too — but only into a home some
/// declaring impl already occupies, so §3's declaration-over-default tier and
/// Gap E's fallback are untouched — and the more specific impl wins, bringing
/// its inherited default with it.
#[test]
fn b73_a_specific_impl_taking_the_trait_default_outranks_a_blanket_declaration() {
    let blanket_first = r#"
        import std::print;

        trait Tag { fun tag(self): i32 { 9 } }

        struct Foo { n: i32 }

        impl type T with Tag { fun tag(self): i32 { 1 } }

        impl Foo with Tag { }

        fun main() { print(Foo { n = 1 }.tag()); }
        "#;
    let specific_first = r#"
        import std::print;

        trait Tag { fun tag(self): i32 { 9 } }

        struct Foo { n: i32 }

        impl Foo with Tag { }

        impl type T with Tag { fun tag(self): i32 { 1 } }

        fun main() { print(Foo { n = 1 }.tag()); }
        "#;
    assert_compiles_and_runs(blanket_first, "9\n");
    assert_compiles_and_runs(specific_first, "9\n");
}

/// §13.2 rows 18–19, and R1's whole point: `spec/types.md` 271–275 says
/// `Conv<Bar>` and `Conv<Baz>` on one subject are two implementations, and B98's
/// pair key agrees — but resolution collapsed them to one home
/// (`member_home_trait` returned a bare trait `Id`), so only the first-declared
/// was ever reachable. Before: the `Baz` annotation reported `Expected Baz, but
/// got Bar instead.` while the `Bar` one compiled and printed `2`.
///
/// It takes R1 *and* R2, against §13.5's reading that R1 alone would do it:
/// splitting the homes is what makes both reachable, and choosing between two
/// reachable homes on one receiver is exactly what the expected type is for.
#[test]
fn b73_two_impls_of_one_trait_at_different_arguments_are_both_reachable() {
    let source = r#"
        import std::print;

        trait Conv<T> { fun conv(self): T; }

        struct Foo { n: i32 }
        struct Bar { n: i32 }
        struct Baz { n: i32 }

        impl Foo with Conv<Bar> { fun conv(self): Bar { Bar { n = 2 } } }

        impl Foo with Conv<Baz> { fun conv(self): Baz { Baz { n = 3 } } }

        fun main() {
            let b: ANNOTATION = Foo { n = 1 }.conv();
            print(b.n);
        }
        "#;
    assert_compiles_and_runs(&source.replace("ANNOTATION", "Baz"), "3\n");
    assert_compiles_and_runs(&source.replace("ANNOTATION", "Bar"), "2\n");
}

/// §13.2 row 20 — TYPE CONFUSION, closed by R1, and the reason deleting std's
/// blanket would not have closed beta trigger (c): there is no blanket in this
/// program. The bound names `Conv<Baz>`, the analyzer types the call as `Baz`,
/// and the transformer's `resolve_member_on_trait_impl` re-dispatched on
/// `trait_ids.contains(&Conv)` alone and emitted the `Conv<Bar>` body: it
/// compiled clean, exited 0, and printed `2` — an `i32` read out of a field
/// declared `str`. R1 carries the bound's own arguments into
/// `bound_dispatch_traits`, so the re-dispatch keys on `Conv<Baz>`.
#[test]
fn b73_a_bound_selects_the_impl_matching_its_trait_arguments() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::string::str;

        trait Conv<T> { fun conv(self): T; }

        struct Foo { n: i32 }
        struct Bar { n: i32 }
        struct Baz { tag: str }

        impl Foo with Conv<Bar> { fun conv(self): Bar { Bar { n = 2 } } }

        impl Foo with Conv<Baz> { fun conv(self): Baz { Baz { tag = "baz" } } }

        fun to_baz<T: Conv<Baz>>(x: T): Baz { x.conv() }

        fun main() {
            let z: Baz = to_baz(Foo { n = 1 });
            print(z.tag);
        }
        "#,
        "baz\n",
    );
}

/// R1's diagnostic (C2). The two homes are named as THIS receiver instantiates
/// them — `Into<Foo>` for the blanket (a USER-written one since §14 deleted
/// std's), `Into<str>` for the specific impl — because `Into` twice tells the
/// reader nothing, and the message says what does select rather than offering
/// an `Into::into` spelling that cannot (B83's "an impossible steer is worse
/// than no steer"). Anchored at the method name (A1/A4), the same span the
/// two-trait ambiguity uses.
#[test]
fn b73_the_argument_ambiguity_names_both_homes_as_the_receiver_instantiates_them() {
    // The fourth `into` in the source is the CALL — the first three are the
    // import path and the two impls' declarations.
    assert_fails_spanning_nth(
        r#"
        import std::print;
        import std::into::Into;
        import std::string::str;

        struct Foo { n: i32 }

        impl type T with Into<T> {
            fun into(self): T { self }
        }

        impl Foo with Into<str> {
            fun into(self): str { "converted" }
        }

        fun main() {
            let s = Foo { n = 1 }.into();
            print(s);
        }
        "#,
        "into",
        3,
        "'into' is ambiguous on 'Foo': both 'Into<Foo>' and 'Into<str>' provide it, and \
         'Into::into' names only the trait, not which of its instantiations; annotate the \
         type this call must produce to pick one",
    );
}

/// R2's ZERO-match leg. An expectation that fits neither home does not get to
/// pick one by being nearest: the call stays ambiguous and says so, rather than
/// resolving to some impl and then reporting a type mismatch against it. That
/// second message would name a home the program never chose. (The `Into<Foo>`
/// home comes from a user-written blanket — the same shape std's deleted
/// blanket gave this pin originally, §14.)
#[test]
fn b73_an_expectation_matching_no_home_leaves_the_call_ambiguous() {
    assert_fails_with(
        r#"
        import std::print;
        import std::into::Into;
        import std::string::str;

        struct Foo { n: i32 }

        impl type T with Into<T> {
            fun into(self): T { self }
        }

        impl Foo with Into<str> {
            fun into(self): str { "converted" }
        }

        fun main() {
            let s: i32 = Foo { n = 1 }.into();
            print(s);
        }
        "#,
        "'into' is ambiguous on 'Foo': both 'Into<Foo>' and 'Into<str>' provide it",
    );
}

/// R1's key from the other side, and the edge case row 20 does not cover: BOTH
/// bounds are written, so each must reach its own impl. Row 20 alone would pass
/// a filter that always picked the LAST impl; this one fails such a filter,
/// because `Conv<Bar>` is the first-declared and `Conv<Baz>` the second.
#[test]
fn b73_two_bounds_at_different_arguments_each_reach_their_own_impl() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::string::str;

        trait Conv<T> { fun conv(self): T; }

        struct Foo { n: i32 }
        struct Bar { n: i32 }
        struct Baz { tag: str }

        impl Foo with Conv<Bar> { fun conv(self): Bar { Bar { n = 2 } } }

        impl Foo with Conv<Baz> { fun conv(self): Baz { Baz { tag = "baz" } } }

        fun to_bar<T: Conv<Bar>>(x: T): Bar { x.conv() }

        fun to_baz<T: Conv<Baz>>(x: T): Baz { x.conv() }

        fun main() {
            print(to_bar(Foo { n = 1 }).n);
            print(to_baz(Foo { n = 1 }).tag);
        }
        "#,
        "2\nbaz\n",
    );
}

/// The permissive half of R1's emission filter, which is what keeps every
/// existing golden byte-identical: an impl whose trait argument is its OWN
/// binder (`impl Box<type T> with Tagged<T>`) is still abstract at the
/// comparison, so the filter proves nothing about it and must keep it. The
/// trait carries a DEFAULT here on purpose — that is what makes the pin
/// non-vacuous: a filter demanding equality drops the impl, and the
/// re-dispatch then silently emits the default (`1`) in place of the override.
#[test]
fn b73_an_impl_whose_trait_argument_is_its_own_binder_survives_the_filter() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        trait Tagged<T> { fun tagged(self): i32 { 1 } }

        struct Box<T> { v: T }

        impl Box<type T> with Tagged<T> { fun tagged(self): i32 { 2 } }

        fun grab<S: Tagged<i32>>(x: S): i32 { x.tagged() }

        fun main() { print(grab(Box { v = 7 })); }
        "#,
        "2\n",
    );
}

/// R3's residue (§13.4(a)(3)), reported at the call site per §13.6 Q4. Two
/// impls bounded by unrelated traits both apply to a `Box<i32>` and neither
/// subsumes the other, so specificity deliberately does not rank them. There is
/// no `Trait::tag(..)` steer — both candidates are the same trait at the same
/// instantiation — so the message states the rule that failed to apply and
/// sends the reader to the two definitions, naming each subject WITH its
/// binder's bound, without which the two render identically.
#[test]
fn b73_two_impls_bounded_by_unrelated_traits_are_an_unrankable_overlap() {
    assert_fails_spanning_nth(
        r#"
        import std::print;
        import std::display::Display;
        import std::compare::Ord;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }

        impl Box<type T: Display> with Tag { fun tag(self): i32 { 2 } }

        impl Box<type U: Ord> with Tag { fun tag(self): i32 { 3 } }

        fun main() { print(Box { v = 5 }.tag()); }
        "#,
        "tag",
        3,
        "'tag' is ambiguous on 'Box<i32>': both 'Box<T> where T: Display' and \
         'Box<U> where U: Ord' provide it and neither impl subject is more specific than \
         the other; vilan picks the more specific of two overlapping impls, so narrow one \
         subject until it is",
    );
}

/// §13.6 Q6's other half, `spec/types.md` §5.4's soundness note: a conditional
/// impl must not let a type satisfy a bound its own binder's bound refuses.
/// Measured both with and without R3's applicability step, this was ALREADY
/// rejected — bound satisfaction goes through `type_implements_trait`, which
/// asks `compare_type`, which resolves a bounded binder to its constraint. The
/// note's own example is stale, for reasons that predate this arc; the pin is
/// here so the claim the spec now makes is gated rather than asserted.
#[test]
fn b73_a_conditional_impl_does_not_satisfy_a_bound_its_binder_refuses() {
    assert_fails_with(
        r#"
        import std::print;

        trait Marker { fun mark(self): i32; }

        struct Wrap<T> { v: T }
        struct Plain { z: i32 }

        impl Wrap<type T: Marker> with Marker { fun mark(self): i32 { 1 } }

        fun need<M: Marker>(m: M): i32 { m.mark() }

        fun main() { print(need(Wrap { v = Plain { z = 1 } })); }
        "#,
        "'Wrap<Plain>' does not implement trait 'Marker'",
    );
}

/// R3's applicability step NARROWS the field and never empties it. With the
/// bounded impl the only one there is, the receiver that fails its bound keeps
/// the bound diagnostic it has always had — which is the right message when no
/// impl fits, and much better than "`Box<Opaque>` has no method 'tag'".
#[test]
fn b73_an_inapplicable_impl_that_is_the_only_one_still_reports_its_bound() {
    assert_fails_with(
        r#"
        import std::print;
        import std::display::Display;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }
        struct Opaque { z: i32 }

        impl Box<type T: Display> with Tag { fun tag(self): i32 { 2 } }

        fun main() { print(Box { v = Opaque { z = 1 } }.tag()); }
        "#,
        "'Opaque' does not implement trait 'Display'",
    );
}

/// Row 15's other order, which the row-15 pin does not cover: the APPLICABLE
/// impl declared first must still win, so the fix cannot be "prefer the later
/// block". Ordering-sensitivity is the whole complaint B73 was filed about.
#[test]
fn b73_an_applicable_impl_wins_whichever_side_of_the_inapplicable_one_it_sits() {
    let applicable_first = r#"
        import std::print;
        import std::display::Display;

        trait Tag { fun tag(self): i32; }

        struct Box<T> { v: T }
        struct Opaque { z: i32 }

        impl Box<type T> with Tag { fun tag(self): i32 { 1 } }

        impl Box<type T: Display> with Tag { fun tag(self): i32 { 2 } }

        fun main() { print(Box { v = Opaque { z = 1 } }.tag()); }
        "#;
    assert_compiles_and_runs(applicable_first, "1\n");
}

/// R3 ranks INSIDE one home and moves no tier (§13.4(a), "Composition with
/// §3"). An inherent `tag` still beats every trait-provided one, including the
/// specific impl row 17's rule would otherwise hand the call to — §13.2 row 23's
/// guarantee, restated for the shape R3 introduced.
#[test]
fn b73_an_inherent_member_still_outranks_the_most_specific_trait_impl() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        trait Tag { fun tag(self): i32 { 9 } }

        struct Foo { n: i32 }

        impl Foo { fun tag(self): i32 { 101 } }

        impl type T with Tag { fun tag(self): i32 { 1 } }

        impl Foo with Tag { }

        fun main() { print(Foo { n = 1 }.tag()); }
        "#,
        "101\n",
    );
}

/// §13.2 row 16, correct before B73 and still correct: a blanket that declares
/// NOTHING contributes no candidate, so the specific impl's own declaration is
/// what runs. R3's new inherited-default candidates must not disturb it — the
/// trait here has no default body to inherit.
#[test]
fn b73_a_blanket_declaring_nothing_leaves_the_specific_impl_alone() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        trait Tag { fun tag(self): i32; }

        struct Foo { n: i32 }

        impl type T with Tag { }

        impl Foo with Tag { fun tag(self): i32 { 7 } }

        fun main() { print(Foo { n = 1 }.tag()); }
        "#,
        "7\n",
    );
}

/// §13.2 rows 21–22, closed by R3 — one program, two answers, which is the
/// sharpest argument for ranking rather than refusing. The direct call resolved
/// through the analyzer's `compare_type` filter, where the blanket matches
/// everything and sorted first, and printed `1`; the bounded call re-dispatches
/// through the transformer's `nominal_matches`, where a generic subject matches
/// nothing, and printed `7`. Half the compiler already preferred the specific
/// impl; R3 is the analyzer agreeing with it. `Tag` takes no arguments, so this
/// is the shape R1/R2 cannot reach — one home, two impls, ranked.
#[test]
fn b73_a_direct_call_and_a_bounded_call_agree_on_which_impl_wins() {
    let direct = r#"
        import std::print;

        trait Tag { fun tag(self): i32; }

        struct Foo { n: i32 }

        impl type T with Tag { fun tag(self): i32 { 1 } }

        impl Foo with Tag { fun tag(self): i32 { 7 } }

        fun main() { print(Foo { n = 1 }.tag()); }
        "#;
    let through_a_bound = r#"
        import std::print;

        trait Tag { fun tag(self): i32; }

        struct Foo { n: i32 }

        impl type T with Tag { fun tag(self): i32 { 1 } }

        impl Foo with Tag { fun tag(self): i32 { 7 } }

        fun show<T: Tag>(x: T): i32 { x.tag() }

        fun main() { print(show(Foo { n = 1 })); }
        "#;
    assert_compiles_and_runs(direct, "7\n");
    assert_compiles_and_runs(through_a_bound, "7\n");
}

// --- B127/B130: std ships no `Into<T>` blanket (method-resolution.md §14) ---
//
// RULED DELETE 2026-08-22 (§14.1). std's `impl type T with Into<T>` was
// selected by resolution at zero sites in the whole tree, the suite included;
// its one working surface was an identity `.into()` nothing called; and its
// one unique affordance — a `T: Into<Foo>` bound fed a `Foo` itself — died in
// an internal compiler error (B130, §14 probe C), because the transformer's
// nominal re-dispatch cannot see a generic subject (§13.3 D3). The trait, the
// module, and the import path all stay; a user who wants identity conversion
// writes the three-line reflexive impl, which is strictly more functional
// than the blanket it replaces (it carries the bound path — probe G). The
// pins below are the deleted world's contract; each `b127_`/the probe-C pin
// was run RED against the pre-deletion tree before the impl was removed.

/// §14 probe B. With no blanket in std, an `into` call reaches only what the
/// program itself implements — no impl, no method. Re-adding a std blanket
/// impl would turn this refusal into a clean compile that prints the receiver.
#[test]
fn b127_an_into_call_with_no_user_impl_is_a_missing_method() {
    assert_fails_with(
        r#"
        import std::print;
        import std::into::Into;

        struct Foo { n: i32 }

        fun main() {
            let f = Foo { n = 1 }.into();
            print(f);
        }
        "#,
        "Foo has no method 'into'",
    );
}

/// §13.2 row 2's FIRST-choice correct answer, reachable at last: one user
/// impl, no annotation, and the call selects it. Std's blanket made every
/// `.into()` receiver a two-home call, so this exact program — the shape
/// `docs/std/strings.md` teaches — was an ambiguity that demanded an
/// annotation (the tax §14 repeals). Red before the deletion, green after.
#[test]
fn b127_an_unannotated_into_call_with_one_user_impl_selects_it() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::into::Into;
        import std::string::str;

        struct Foo { n: i32 }

        impl Foo with Into<str> {
            fun into(self): str { "converted" }
        }

        fun main() {
            let s = Foo { n = 1 }.into();
            print(s);
        }
        "#,
        "converted\n",
    );
}

/// B130, closed by the deletion. A `T: Into<Foo>` bound fed a `Foo` was the
/// blanket's one unique affordance, and it died with "internal: a call
/// resolved to 'Into''s requirement 'into', which has no body … please report
/// this program" anchored at the import line (live in released 0.34.0): the
/// analyzer accepted the bound through the blanket, and the transformer's
/// `nominal_matches` re-dispatch cannot see a generic subject (§13.3 D3), so
/// the no-body guard fired. With no blanket the bound is refused cleanly at
/// the call, with the note at the bound's declaration. This pin is the red
/// half of the plant: against the pre-deletion tree it fails on the internal
/// error where the refusal should be.
#[test]
fn b130_an_into_bound_fed_its_target_without_an_impl_is_refused_cleanly() {
    let source = r#"
        import std::print;
        import std::into::Into;

        struct Foo { n: i32 }

        fun accept<T: Into<Foo>>(x: T): Foo { x.into() }

        fun main() { print(accept(Foo { n = 7 }).n); }
        "#;
    assert_fails_with(
        source,
        "'Foo' does not implement trait 'Into<Foo>', required by a generic bound of this call",
    );
    assert_fails_noting(
        source,
        "'Foo' does not implement trait 'Into<Foo>'",
        "T",
        "the bound is declared here",
    );
}

/// §14 probe G, the migration path: a user-written reflexive impl delivers
/// what the blanket only promised. It satisfies the `T: Into<Foo>` bound AND
/// carries the bound path's re-dispatch, because a nominal subject is visible
/// where a generic one never was (§13.3 D3) — measured identical with the
/// blanket still in std, which is what makes the three-line impl "strictly
/// more functional" than the five lines it replaces (§14, migration).
#[test]
fn b130_a_user_reflexive_impl_carries_the_bound_path() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::into::Into;

        struct Foo { n: i32 }

        impl Foo with Into<Foo> {
            fun into(self): Foo { self }
        }

        fun accept<T: Into<Foo>>(x: T): Foo { x.into() }

        fun main() { print(accept(Foo { n = 7 }).n); }
        "#,
        "7\n",
    );
}

// --- B128: R2 selecting an unrankable home reports it (§13.8's residue) -----
//
// `rank_member_candidates` used to hand R2 one REPRESENTATIVE per home — an
// unranked home was stood in for by its first maximum — so when the expected
// type selected a home R3's specificity order could not rank, the first
// maximum answered silently where the home's own `AmbiguousImpls` report
// should. The shape needs BOTH an argument-distinct split (so R2 runs at all)
// AND an unrankable overlap inside the selected home; no §13.2 row and no
// program in the tree had it, which is why §13.8 shipped with it deferred.

/// The probe the tree lacked, and B128's close. `Conv<Bar>`'s home holds two
/// impls bounded by unrelated traits (`Box<i32>` satisfies both, neither
/// subsumes — the row-214 shape); `Conv<str>`'s home is the argument-distinct
/// split that routes the call through R2. The `Bar` annotation selects the
/// unrankable home, and the call must report that home's overlap — before the
/// fix this compiled cleanly and printed `1`, the first maximum by
/// declaration order, which is the exact order-dependence B73 was filed over.
#[test]
fn b128_an_expectation_selecting_an_unrankable_home_reports_its_overlap() {
    assert_fails_with(
        r#"
        import std::print;
        import std::display::Display;
        import std::compare::Ord;
        import std::string::str;

        trait Conv<T> { fun conv(self): T; }

        struct Box<T> { v: T }
        struct Bar { n: i32 }

        impl Box<type T: Display> with Conv<Bar> { fun conv(self): Bar { Bar { n = 1 } } }

        impl Box<type U: Ord> with Conv<Bar> { fun conv(self): Bar { Bar { n = 2 } } }

        impl Box<type T> with Conv<str> { fun conv(self): str { "s" } }

        fun main() {
            let b: Bar = Box { v = 5 }.conv();
            print(b.n);
        }
        "#,
        "'conv' is ambiguous on 'Box<i32>': both 'Box<T> where T: Display' and \
         'Box<U> where U: Ord' provide it and neither impl subject is more specific than \
         the other",
    );
}

/// The complement that keeps the fix honest: the same program, with the
/// expectation selecting the RANKED home instead. An unrankable overlap the
/// call does not select must not contaminate it — the `str` annotation picks
/// `Conv<str>`'s single impl and the program runs.
#[test]
fn b128_an_expectation_selecting_a_ranked_home_beside_an_unrankable_one_runs() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::display::Display;
        import std::compare::Ord;
        import std::string::str;

        trait Conv<T> { fun conv(self): T; }

        struct Box<T> { v: T }
        struct Bar { n: i32 }

        impl Box<type T: Display> with Conv<Bar> { fun conv(self): Bar { Bar { n = 1 } } }

        impl Box<type U: Ord> with Conv<Bar> { fun conv(self): Bar { Bar { n = 2 } } }

        impl Box<type T> with Conv<str> { fun conv(self): str { "s" } }

        fun main() {
            let s: str = Box { v = 5 }.conv();
            print(s);
        }
        "#,
        "s\n",
    );
}

// --- A25: remote sources — subscribe by demand, unsubscribe at zero ---------
//
// RATIFIED 2026-08-19 and shipped (`proposal/remote-sources.md` §2, §5,
// §8). Every observing path on a `RemoteSource` takes a COUNTED lease: the
// 0→1 transition sends `Subscribe` (eagerly — the server's immediate
// current-value `Update` is how the mirror seeds), the 1→0 transition sends
// `Unsubscribe` (deferred to the turn's settle via `at_settle`, so a
// same-turn re-subscribe cancels it and a dispose-and-rebuild re-render
// churns no frames). `get`/`status` are passive; `map`/`or` confront the
// `Option` once and ride the ambient owner.
//
// Before A25, the client's only control frame builder was `encode_control`
// and both its call sites passed `"Subscribe"`: nothing ever constructed an
// `Unsubscribe`, the server's `stop` was unreachable, and `RemoteSource::sub`
// handed back the LOCAL cache's `Subscription` — disposing it stopped the
// observer, never the channel. The "before" comments on pins A–C record what
// each program printed then, so none of them is vacuous.
//
// The harness is a frame-logging relay between two `duplex_pair` ends: every
// frame is printed before it is forwarded — the cheapest wire tap the tree
// allows, and the first place the suite observes a reactive frame at all.

/// §1.1 — the headline. Before A25 the last line was `down {"Update":[0,2]}`:
/// the server kept forwarding to a client that had disposed its only
/// subscription, and `remote.get()` afterwards still read `Some(2)`. Under
/// the count, the 1→0 transition sends `Unsubscribe` (inline here: no
/// ambient turn) and the post-dispose `set` puts nothing on the wire at all.
#[test]
fn a25_disposing_the_last_remote_subscription_sends_unsubscribe() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun main() {
            // A logging relay between the two ends: client <-> spy <-> server.
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)}");
                spy_server.send(frame);
            });
            spy_server.on_frame(|frame| {
                print(i"down {text_of(frame)}");
                spy_client.send(frame);
            });

            let counter: Signal<i32> = Signal::new(0);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            let watching = remote.sub(|_n| {});
            counter.set(1);
            watching.dispose();
            counter.set(2);
        }
        "#,
        "up   {\"Subscribe\":0}\n\
         down {\"Update\":[0,0]}\n\
         down {\"Update\":[0,1]}\n\
         up   {\"Unsubscribe\":0}\n",
    );
}

/// §1.2 — the defect that fell out of the same probe. Before A25
/// `ReactiveServer::start` pushed a NEW live forward per `Subscribe`, and
/// `RemoteSource::sub` sent one on EVERY call, so two local watchers opened two
/// server-side forwards on one channel: this printed ten lines — `A sees 1`
/// twice (B's `Subscribe` re-delivered the current value to A), then `A sees
/// 2` / `B sees 2` twice (two Update frames), and `A sees 3` twice even after
/// B disposed, because the dispose closed nothing. Under the count a second
/// `sub` sends no frame at all when the count is already ≥1, and B's dispose
/// (2→1) sends none either.
#[test]
fn a25_a_second_watcher_opens_no_second_server_forward() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let (client_end, server_end) = duplex_pair();
            let counter: Signal<i32> = Signal::new(0);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            let first = remote.sub(|n| print(i"A sees {n}"));
            counter.set(1);
            let second = remote.sub(|n| print(i"B sees {n}"));
            counter.set(2);
            second.dispose();
            counter.set(3);
            first.dispose();
        }
        "#,
        "A sees 0\n\
         A sees 1\n\
         B sees 1\n\
         A sees 2\n\
         B sees 2\n\
         A sees 3\n",
    );
}

/// §3 — the server's half of §1.2, independent of the client: `start` is
/// idempotent. A raw client that says `Subscribe` twice on one channel gets
/// ONE forward (one `Update` per change, no re-seed for the duplicate), and
/// one `Unsubscribe` stops it. Before A25 the second `Subscribe` opened a
/// second forward: `down {"Update":[0,0]}` twice, then `down
/// {"Update":[0,1]}` twice.
#[test]
fn a25_a_second_subscribe_frame_opens_no_second_forward() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveServer, duplex_pair, encode_control };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun main() {
            let (client_end, server_end) = duplex_pair();
            let counter: Signal<i32> = Signal::new(0);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            client_end.on_frame(|frame| print(i"down {text_of(frame)}"));
            client_end.send(encode_control(json_codec(), "Subscribe", channel));
            client_end.send(encode_control(json_codec(), "Subscribe", channel));
            counter.set(1);
            client_end.send(encode_control(json_codec(), "Unsubscribe", channel));
            counter.set(2);
            print("done");
        }
        "#,
        "down {\"Update\":[0,0]}\n\
         down {\"Update\":[0,1]}\n\
         done\n",
    );
}

/// §2a — the case the deferral buys: `sub` + `dispose` + `sub` inside ONE
/// turn is exactly one `Subscribe` and no `Unsubscribe`. The first `sub`
/// sends its `Subscribe` eagerly (the value lands, A fires); the dispose
/// (1→0) marks the mirror closing and defers the `Unsubscribe` to the turn's
/// settle; the second `sub` (0→1) finds the close pending, cancels it, and
/// sends nothing — the channel never closed server-side. After the settle
/// the channel is still live: the `set` reaches B.
#[test]
fn a25_a_same_turn_resubscribe_cancels_the_pending_unsubscribe() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::{ Signal, batch };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun main() {
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)}");
                spy_server.send(frame);
            });
            spy_server.on_frame(|frame| {
                print(i"down {text_of(frame)}");
                spy_client.send(frame);
            });

            let counter: Signal<i32> = Signal::new(0);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            batch(|| {
                let first = remote.sub(|n| print(i"A sees {n}"));
                first.dispose();
                let _second = remote.sub(|n| print(i"B sees {n}"));
            });
            print("settled");
            counter.set(1);
        }
        "#,
        "up   {\"Subscribe\":0}\n\
         down {\"Update\":[0,0]}\n\
         A sees 0\n\
         B sees 0\n\
         settled\n\
         down {\"Update\":[0,1]}\n\
         B sees 1\n",
    );
}

/// §2a/§3 — a pending close cannot cross a rebind. The last lease goes inside
/// a turn (the `Unsubscribe` is owed, deferred to settle); a reconnect lands
/// in the same turn and `rebind`s the mirror to a fresh channel. The pending
/// close named the OLD channel on a dead connection, so `rebind` drops it:
/// nothing flushes at settle (without the clear, the flush would send
/// `{"Unsubscribe":99}` — the NEW channel's id — before `settled`). Nothing
/// is watched, so the rebind sends no `Subscribe` either; the next `sub`
/// subscribes on the new channel and its dispose closes it.
#[test]
fn a25_a_pending_unsubscribe_does_not_cross_a_rebind() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::{ Signal, batch };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair, encode_control };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun main() {
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)}");
                spy_server.send(frame);
            });
            spy_server.on_frame(|frame| {
                print(i"down {text_of(frame)}");
                spy_client.send(frame);
            });

            let counter: Signal<i32> = Signal::new(0);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            batch(|| {
                let watching = remote.sub(|n| print(i"sees {n}"));
                watching.dispose();
                remote.rebind(99, encode_control(json_codec(), "Subscribe", 99));
            });
            print("settled");
            let again = remote.sub(|n| print(i"sees {n}"));
            again.dispose();
        }
        "#,
        "up   {\"Subscribe\":0}\n\
         down {\"Update\":[0,0]}\n\
         sees 0\n\
         settled\n\
         up   {\"Subscribe\":99}\n\
         sees 0\n\
         up   {\"Unsubscribe\":99}\n",
    );
}

/// §2a — `Subscription.release` runs ONCE. A counted subscription disposed by
/// hand and then again by the owner it was taken into decrements a single
/// time: with `second` still watching, the count is 1 after both disposes,
/// and only `second`'s own dispose closes the channel. A double decrement
/// would put the `Unsubscribe` before `owner disposed`.
#[test]
fn a25_a_counted_subscription_releases_its_lease_once() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::{ Owner, Signal };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun main() {
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)}");
                spy_server.send(frame);
            });
            spy_server.on_frame(|frame| spy_client.send(frame));

            let counter: Signal<i32> = Signal::new(0);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            let owner = Owner::new();
            let first = owner.take(remote.sub(|_n| {}));
            let second = remote.sub(|_n| {});
            first.dispose();
            print("first disposed by hand");
            owner.dispose();
            print("owner disposed");
            second.dispose();
            print("second disposed");
        }
        "#,
        "up   {\"Subscribe\":0}\n\
         first disposed by hand\n\
         owner disposed\n\
         up   {\"Unsubscribe\":0}\n\
         second disposed\n",
    );
}

/// §2b/§2c/§2d — the ratified surface, in one program. Before A25 this did
/// not compile: "cannot find 'Status' in the imported path" and
/// "RemoteSource<i32> has no method 'map'". It pins four things at once:
/// `status()` is passive (it reads `Waiting` with no frame on the wire and
/// needs no owner); `map` carries a fallback of a DIFFERENT type than `T`
/// (`str` from an `i32` mirror); the count rides ownership (one `Subscribe`
/// when the scope's `map` takes its lease, one `Unsubscribe` when the scope
/// is disposed); and the owner-coverage fence propagates through a plain call
/// — `label` is one function call down from `owner_scope.run` and compiles,
/// where the same call outside any scope is a hard error (the next pins).
#[test]
fn a25_map_carries_a_fallback_and_the_count_rides_the_owner() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::option::Option::{ None, Some, self };
        import std::print;
        import std::reactive::{ Owner, Signal, owner_scope };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, Status, duplex_pair };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        // One plain call down from the scope — coverage propagates here.
        fun label(mirror: RemoteSource<i32>): Signal<str> {
            mirror.map(|value| match value {
                Some(let n) => i"{n}",
                None => "Loading...",
            })
        }

        fun main() {
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)}");
                spy_server.send(frame);
            });
            spy_server.on_frame(|frame| {
                print(i"down {text_of(frame)}");
                spy_client.send(frame);
            });

            let counter: Signal<i32> = Signal::new(7);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            // Passive: no lease, no owner, no frame.
            match remote.status().get() {
                Status::Waiting => print("status = Waiting"),
                Status::Ready => print("status = Ready"),
            }

            let scope = Owner::new();
            let text = owner_scope.run(scope, || label(remote));
            print(i"text = {text.get()}");
            counter.set(8);
            print(i"text = {text.get()}");
            scope.dispose();
            counter.set(9);
            print(i"text = {text.get()}");
        }
        "#,
        "status = Waiting\n\
         up   {\"Subscribe\":0}\n\
         down {\"Update\":[0,7]}\n\
         text = 7\n\
         down {\"Update\":[0,8]}\n\
         text = 8\n\
         up   {\"Unsubscribe\":0}\n\
         text = 8\n",
    );
}

/// §2b — `map` requires an ambient owner, statically: a network subscription
/// must have an owner, and `get_owner()` inside `map` is a strict context
/// read, so calling it from `main` (the run-less root) is the coverage error.
#[test]
fn a25_map_outside_an_owner_scope_is_a_compile_error() {
    assert_fails_with(
        r#"
        import std::json::json_codec;
        import std::option::Option::{ None, Some, self };
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let (client_end, server_end) = duplex_pair();
            let counter: Signal<i32> = Signal::new(7);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let text = remote.map(|value| match value {
                Some(let n) => i"{n}",
                None => "Loading...",
            });
            print(text.get());
        }
        "#,
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
}

/// §2c — `or` IS `map`, so it carries the same law: no owner, no compile.
#[test]
fn a25_or_outside_an_owner_scope_is_a_compile_error() {
    assert_fails_with(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let (client_end, server_end) = duplex_pair();
            let counter: Signal<i32> = Signal::new(7);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let text = remote.or(0);
            print(text.get());
        }
        "#,
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
}

/// E74 (diagnostics-standard A2): §2b's fence anchors at the USER'S `map`
/// call — the strict read it trips sits in std (`get_owner`, reached from
/// `RemoteSource::map`), which is where the diagnostic anchored before the
/// walk-back; the std read is now the C3 note.
#[test]
fn e74_a25_map_anchors_at_the_users_call() {
    let source = r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let (client_end, server_end) = duplex_pair();
            let counter: Signal<i32> = Signal::new(7);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let text = remote.map(|value| "seen");
            print(text.get());
        }
        "#;
    assert_fails_spanning(
        source,
        r#"remote.map(|value| "seen")"#,
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_only_failure_noting_into_std(
        source,
        "without an enclosing `run`",
        "the read is inside `get_owner` here",
    );
}

/// E74, §2c's shape: `or` IS `map`, so the walk-back crosses TWO std frames
/// (`or` → `map` → `get_owner`) and still lands on the user's `.or` call.
#[test]
fn e74_a25_or_anchors_at_the_users_call() {
    let source = r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let (client_end, server_end) = duplex_pair();
            let counter: Signal<i32> = Signal::new(7);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let text = remote.or(0);
            print(text.get());
        }
        "#;
    assert_fails_spanning(
        source,
        "remote.or(0)",
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_only_failure_noting_into_std(
        source,
        "without an enclosing `run`",
        "the read is inside `get_owner` here",
    );
}

/// §2c — `or` reads `initial` before the first frame and the mirrored value
/// after. Over an in-process transport the server's seed `Update` lands
/// synchronously on `Subscribe`, so the relay HOLDS upstream frames until the
/// program releases them: `or` is read with the `Subscribe` still in hand
/// (`(pending)`), then the frame goes through and the derived signal follows
/// the cache — the seed, then a later change. The scope's dispose sends the
/// `Unsubscribe` (held too, so it prints but never reaches the server).
#[test]
fn a25_or_reads_the_initial_before_the_first_frame_and_the_value_after() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::{ Owner, Signal, owner_scope };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
        import std::shared::Shared;
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun main() {
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            let held: Shared<List<Frame>> = Shared::new([]);
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)} (held)");
                held.write().push(frame);
            });
            spy_server.on_frame(|frame| {
                print(i"down {text_of(frame)}");
                spy_client.send(frame);
            });

            let title: Signal<str> = Signal::new("hello");
            let channel = ReactiveServer::new(server_end, json_codec()).expose(title);
            let remote: RemoteSource<str> = ReactiveClient::new(client_end, json_codec()).source(channel);

            let scope = Owner::new();
            let shown = owner_scope.run(scope, || remote.or("(pending)"));
            print(i"before the first frame: {shown.get()}");
            for frame in held.read() {
                spy_server.send(frame);
            }
            print(i"after the first frame: {shown.get()}");
            title.set("hello again");
            print(i"after a change: {shown.get()}");
            scope.dispose();
        }
        "#,
        "up   {\"Subscribe\":0} (held)\n\
         before the first frame: (pending)\n\
         down {\"Update\":[0,\"hello\"]}\n\
         after the first frame: hello\n\
         down {\"Update\":[0,\"hello again\"]}\n\
         after a change: hello again\n\
         up   {\"Unsubscribe\":0} (held)\n",
    );
}

/// §2b — two `map`s under one owner take ONE lease each on one count: one
/// `Subscribe` for both (the second finds the count at 1 and sends nothing),
/// both derived signals follow the mirror, and the owner's dispose releases
/// both leases — one `Unsubscribe`, after which neither moves.
#[test]
fn a25_two_maps_under_one_owner_take_one_subscribe() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::option::Option::{ None, Some, self };
        import std::print;
        import std::reactive::{ Owner, Signal, owner_scope };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun main() {
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)}");
                spy_server.send(frame);
            });
            spy_server.on_frame(|frame| {
                print(i"down {text_of(frame)}");
                spy_client.send(frame);
            });

            let counter: Signal<i32> = Signal::new(1);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            let scope = Owner::new();
            let (doubled, label) = owner_scope.run(scope, || {
                let doubled = remote.map(|value| match value {
                    Some(let n) => n * 2,
                    None => 0,
                });
                let label = remote.map(|value| match value {
                    Some(let n) => i"n={n}",
                    None => "n=?",
                });
                (doubled, label)
            });
            print(i"{doubled.get()} {label.get()}");
            counter.set(2);
            print(i"{doubled.get()} {label.get()}");
            scope.dispose();
            counter.set(3);
            print(i"{doubled.get()} {label.get()}");
        }
        "#,
        "up   {\"Subscribe\":0}\n\
         down {\"Update\":[0,1]}\n\
         2 n=1\n\
         down {\"Update\":[0,2]}\n\
         4 n=2\n\
         up   {\"Unsubscribe\":0}\n\
         4 n=2\n",
    );
}

/// §2d — the honest sentence, pinned: a `status` observer ALONE puts nothing
/// on the wire and stays `Waiting` through a server-side change, because
/// `status` reports and does not ask — the channel was never opened. Only a
/// real observer (`sub`) subscribes; then the cache fills and `status`
/// follows it to `Ready`.
#[test]
fn a25_status_alone_opens_nothing_and_stays_waiting() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::Signal;
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, Status, duplex_pair };
        import std::wire::Frame;

        fun text_of(frame: Frame): str {
            match frame {
                Frame::Text(let text) => text,
                Frame::Binary(let _bytes) => "<binary>",
            }
        }

        fun describe(status: Status): str {
            match status {
                Status::Waiting => "Waiting",
                Status::Ready => "Ready",
            }
        }

        fun main() {
            let (client_end, spy_client) = duplex_pair();
            let (spy_server, server_end) = duplex_pair();
            spy_client.on_frame(|frame| {
                print(i"up   {text_of(frame)}");
                spy_server.send(frame);
            });
            spy_server.on_frame(|frame| {
                print(i"down {text_of(frame)}");
                spy_client.send(frame);
            });

            let counter: Signal<i32> = Signal::new(1);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(counter);
            let remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);

            let watching_status = remote.status().sub(|status| print(i"status: {describe(status)}"));
            counter.set(2);
            print(i"after a change: {describe(remote.status().get())}");
            let watching = remote.sub(|n| print(i"sees {n}"));
            watching.dispose();
            watching_status.dispose();
        }
        "#,
        "status: Waiting\n\
         after a change: Waiting\n\
         up   {\"Subscribe\":0}\n\
         down {\"Update\":[0,2]}\n\
         status: Ready\n\
         sees 2\n\
         up   {\"Unsubscribe\":0}\n",
    );
}

/// B129 (found by A25's lane; repro'd on v0.30.0): an empty `[]` passed to a
/// `T`-typed parameter did not take its element type from the receiver's
/// already-bound `T` — `remote.or([])` on a `RemoteSource<List<Todo>>` gave a
/// `Signal<List<unknown>>` ("cannot access field 'done' on type any" at the
/// first use), and `Option<List<Todo>>::unwrap_or([])` lost it the same way.
/// Fixed in the analyzer's empty-list arm: the literal's element slot grounds
/// from the expectation (`List<E>`, `E` fully determined). This pin asserts
/// the UNANNOTATED `or([])` with the element reaching field access.
///
/// The pin's ORIGINAL body consumed `items` through a second
/// `items.map(|list| ..)` — which stacks a SECOND, independent gap on top of
/// B129's: a `.map` on a `let`-bound signal freezes its closure parameter
/// before the receiver's binding lands (B125/P21's solver-ordering family;
/// it reproduces with a NON-empty initial and no `[]` anywhere, so it was
/// never B129's). That shape is pinned separately below,
/// `b129_a_map_on_a_let_bound_signal_types_its_closure_parameter`, still
/// ignored; this body consumes the mirror through `get()` instead.
#[test]
fn a25_or_of_an_empty_list_infers_the_element_type_without_an_annotation() {
    assert_compiles_and_runs(
        r#"
        import std::json::json_codec;
        import std::print;
        import std::reactive::{ Owner, Signal, owner_scope };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        [derive(Wire, PartialEq, Debug)]
        struct Todo { id: i32, done: bool }

        fun open_count(remote: RemoteSource<List<Todo>>): i32 {
            let items = remote.or([]);
            let list = items.get();
            mut open = 0;
            for todo in list {
                if !todo.done {
                    open += 1;
                }
            }
            open
        }

        fun main() {
            let (client_end, server_end) = duplex_pair();
            let todos: Signal<List<Todo>> = Signal::new([Todo { id = 1, done = false }, Todo { id = 2, done = true }]);
            let channel = ReactiveServer::new(server_end, json_codec()).expose(todos);
            let remote: RemoteSource<List<Todo>> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let scope = Owner::new();
            print(owner_scope.run(scope, || open_count(remote)));
            scope.dispose();
        }
        "#,
        "1\n",
    );
}

/// B129: `unwrap_or([])` on an `Option<List<Note>>` — the same shape without
/// any reactive machinery. The receiver binds the impl's `T` to `List<Note>`;
/// the empty argument grounds its element slot from it, so the result
/// iterates and reaches fields on BOTH sides of the option.
#[test]
fn b129_unwrap_or_of_an_empty_list_takes_the_element_from_the_receiver() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::{ Option, Some, None };

        [derive(PartialEq, Debug)]
        struct Note { id: i32, done: bool }

        fun open_count(maybe: Option<List<Note>>): i32 {
            let notes = maybe.unwrap_or([]);
            mut open = 0;
            for note in notes {
                if !note.done {
                    open += 1;
                }
            }
            open
        }

        fun main() {
            print(open_count(None));
            print(open_count(Some([Note { id = 1, done = false }, Note { id = 2, done = true }])));
        }
        "#,
        "0\n1\n",
    );
}

/// B129 family, match-arm result position: a `None => []` leg takes its
/// element type from the sibling leg's `List<Note>` through the match's leg
/// unification, and the merged value reaches fields. (This shape already
/// worked before the B129 fix — the legs unify — and is pinned so it stays
/// working; the empty leg's OWN slot grounding is the previous pins'.)
#[test]
fn b129_a_none_arm_empty_list_takes_the_type_from_the_sibling_arm() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::{ Option, Some, None };

        [derive(PartialEq, Debug)]
        struct Note { id: i32, done: bool }

        fun open_count(maybe: Option<List<Note>>): i32 {
            let notes = match maybe {
                Some(let list) => list,
                None => [],
            };
            mut open = 0;
            for note in notes {
                if !note.done {
                    open += 1;
                }
            }
            open
        }

        fun main() {
            print(open_count(None));
            print(open_count(Some([Note { id = 1, done = false }])));
        }
        "#,
        "0\n1\n",
    );
}

/// B129, match-arm result position under a DECLARED return type: when every
/// leg is an empty `[]`, no sibling leg can supply the element — the
/// function's declared `List<Note>` is the expectation that reaches the legs
/// (`expected_types` flows into each leg body), and the returned list pushes
/// and reads as `List<Note>`. A guard pin: the declared return type carries
/// this shape with or without the slot grounding (planting the B129 fix out
/// leaves it green), so it pins the flow, not the fix.
#[test]
fn b129_a_match_of_only_empty_lists_takes_the_declared_return_type() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        [derive(PartialEq, Debug)]
        struct Note { id: i32, done: bool }

        fun fallback(flag: bool): List<Note> {
            match flag {
                true => [],
                false => [],
            }
        }

        fun main() {
            mut notes = fallback(true);
            notes.push(Note { id = 7, done = false });
            print(notes[0].id);
        }
        "#,
        "7\n",
    );
}

/// B129 negative: an empty `[]` with NO expected type anywhere is still the
/// ambiguity it always was, with the same message — the grounding only fires
/// under a determined `List<E>` expectation.
#[test]
fn b129_an_empty_list_with_no_expected_type_still_errors() {
    assert_fails_with(
        r#"
        import std::print;

        fun main() {
            let things = [];
            print(things[0]);
        }
        "#,
        "its element type is never determined",
    );
}

/// B129 negative: an empty `[]` bound to a FREE generic (`head<T>(xs:
/// List<T>)` with nothing else fixing `T`) must not ground — `T` stays
/// abstract and a field access on it errors as before. A fully determined
/// expectation is the grounding's precondition; an abstract one defers.
#[test]
fn b129_an_empty_list_argument_binding_a_free_generic_still_errors() {
    assert_fails_with(
        r#"
        import std::print;

        [derive(PartialEq, Debug)]
        struct Note { id: i32, done: bool }

        fun head<T>(xs: List<T>): T {
            xs[0]
        }

        fun main() {
            print(head([]).id);
        }
        "#,
        "cannot access field 'id' on type T",
    );
}

/// The OTHER gap the original A25 pin's body carried, isolated — and, closed,
/// re-diagnosed (B125's lane, type-solver.md "What B129's second gap actually
/// was"): it was never P21's family and never about the `let`. The closure
/// parameter `list` is filled fine once `.map` resolves; what went wrong is
/// that `for todo in list` resolved FIRST — `ForEachItem` sits at priority 8,
/// the `.map` call deferred to the next pass because its receiver had not
/// grounded yet, and the for-each resolver committed the item to `any` on
/// sight of an `Unknown` iterable instead of deferring on an unknown closure
/// parameter the way the field-accessor, method-call, call-subject and match
/// resolvers already do. Annotating `items` only worked because it let the
/// call resolve at priority 6 of the first pass, ahead of the loop; the
/// inline chain failed the same way (its receiver is a call). The resolver
/// now defers; `resolve_subscript` got the same rule.
#[test]
fn b129_a_map_on_a_let_bound_signal_types_its_closure_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::reactive::{ Owner, Signal, owner_scope };

        [derive(PartialEq, Debug)]
        struct Todo { id: i32, done: bool }

        fun main() {
            let scope = Owner::new();
            let n = owner_scope.run(scope, || {
                let items = Signal::new([Todo { id = 1, done = false }]);
                let remaining: Signal<i32> = items.map(|list| {
                    mut open = 0;
                    for todo in list {
                        if !todo.done {
                            open += 1;
                        }
                    }
                    open
                });
                remaining.get()
            });
            print(n);
            scope.dispose();
        }
        "#,
        "1\n",
    );
}

/// The same defect on `List` with no signal and no `let` in the way beyond the
/// receiver's own: indexing the parameter (`resolve_subscript` reported
/// "cannot index unknown" on the first pass, before the call filled it).
#[test]
fn b129_indexing_an_unannotated_closure_parameter_waits_for_its_owning_call() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        struct Todo { id: i32, done: bool }

        fun main() {
        	let items = [[Todo { id = 1, done = false }]];
        	let ids: List<i32> = items.map(|list| list[0].id);
        	print(ids[0]);
        }
        "#,
        "1\n",
    );
}

/// Iterating it — the for-each shape of the todo example, on a plain nested
/// list whose receiver grounds only at priority 10 of the first pass.
#[test]
fn b129_iterating_an_unannotated_closure_parameter_waits_for_its_owning_call() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        struct Todo { id: i32, done: bool }

        fun main() {
        	let items = [[Todo { id = 1, done = false }, Todo { id = 2, done = true }]];
        	let remaining: List<i32> = items.map(|list| {
        		mut open = 0;
        		for todo in list {
        			if !todo.done {
        				open += 1;
        			}
        		}
        		open
        	});
        	print(remaining[0]);
        }
        "#,
        "1\n",
    );
}

/// The inline chain the old pin's comment said worked — it did not (the
/// receiver is a call, resolved at priority 11, so the loop ran first just
/// the same), and it is pinned here so the claim is checked rather than
/// remembered.
#[test]
fn b129_the_inline_chain_types_its_closure_parameter_too() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::reactive::{ Owner, Signal, owner_scope };

        struct Todo { id: i32, done: bool }

        fun main() {
        	let scope = Owner::new();
        	let n = owner_scope.run(scope, || {
        		let remaining = Signal::new([Todo { id = 1, done = false }]).map(|list| {
        			mut open = 0;
        			for todo in list {
        				if !todo.done {
        					open += 1;
        				}
        			}
        			open
        		});
        		remaining.get()
        	});
        	print(n);
        	scope.dispose();
        }
        "#,
        "1\n",
    );
}

/// The bound on the defer: a parameter NO call ever fills stays open to the
/// end of the fixpoint and reports — it is not silently accepted. Indexing
/// it used to report "cannot index unknown" on the first pass; iterating it
/// used to type the item `any` and compile clean (the `any` leaking out of
/// an untyped parameter). Both reported through the leftover sweep at first
/// ("could not be resolved" at the use); since B131 the report is the
/// parameter-anchored refusal (the b131_* pins below own its anchor, its
/// one-per-root-cause count, and the called-closure complement).
#[test]
fn b129_a_never_called_closures_parameter_still_reports() {
    assert_fails_with(
        r#"
        import std::print;

        fun main() {
        	let first = |xs| xs[0];
        	print(1);
        }
        "#,
        "is never given a type",
    );
    assert_fails_with(
        r#"
        import std::print;

        fun main() {
        	let walk = |xs| {
        		for x in xs {
        			print(x);
        		}
        	};
        	print(1);
        }
        "#,
        "is never given a type",
    );
}

/// B131 — the head and the anchor. B13's rule names the invisible decision
/// ("inferred from the closure's first call"); a closure that is NEVER
/// called has no such call, so nothing can ever fill its unannotated
/// parameter and every use of it stalls. The refusal names the parameter
/// and anchors AT the parameter — the one place the fix (an annotation)
/// goes — instead of surfacing as the leftover sweep's "could not be
/// resolved" at whichever use happened to stall.
#[test]
fn b131_a_never_called_closures_parameter_reports_at_the_parameter() {
    // The indexing shape: the sweep used to report "type of variable 'first'
    // could not be resolved" over the whole `let`.
    assert_fails_spanning(
        r#"
        import std::print;

        fun main() {
        	let first = |xs| xs[0];
        	print(1);
        }
        "#,
        "xs",
        "`xs` is never given a type: this closure is never called and its parameter is \
         unannotated; annotate it (e.g. `|xs: List<i32>|`)",
    );
    // The iterating shape: the sweep used to report "type of function call
    // arguments could not be resolved" at `print(x)` — the use, two hops
    // from the cause.
    assert_fails_spanning(
        r#"
        import std::print;

        fun main() {
        	let walk = |xs| {
        		for x in xs {
        			print(x);
        		}
        	};
        	print(1);
        }
        "#,
        "xs",
        "`xs` is never given a type: this closure is never called and its parameter is \
         unannotated; annotate it (e.g. `|xs: List<i32>|`)",
    );
}

/// B131 × B5 — one diagnostic per root cause: the parameter refusal replaces
/// the leftover sweep's residuals at the uses rather than stacking on top of
/// them (the residuals stay silent behind it exactly as behind any real
/// diagnostic).
#[test]
fn b131_the_leftover_sweep_stays_silent_behind_the_parameter_refusal() {
    let source = r#"
        import std::print;

        fun main() {
        	let walk = |xs| {
        		for x in xs {
        			print(x);
        		}
        	};
        	print(1);
        }
        "#;
    assert_fails_once_with(source, "is never given a type");
    assert_fails_without(source, "could not be resolved");
}

/// B131's complement — a closure that IS called still infers its parameter
/// from the first call (B13) and stays refusal-free.
#[test]
fn b131_a_called_closure_still_infers_from_its_first_call() {
    assert_compiles_and_runs(
        r#"
        import std::print;

        fun main() {
        	let double = |x| x * 2;
        	print(double(3));
        }
        "#,
        "6\n",
    );
}

/// B131, the multi-parameter edge: only the parameter whose uses stalled is
/// named — a sibling parameter with no stalled use raises nothing, so the
/// refusal count stays one per root cause.
#[test]
fn b131_only_the_starved_parameter_is_named() {
    let source = r#"
        import std::print;

        fun main() {
        	let pick = |rows, fallback| rows[0];
        	print(1);
        }
        "#;
    assert_fails_once_with(source, "is never given a type");
    assert_fails_spanning(
        source,
        "rows",
        "`rows` is never given a type: this closure is never called and its parameter is \
         unannotated; annotate it (e.g. `|rows: List<i32>|`)",
    );
}

// --- std::markdown — the census parser's construct pins (proposal/markdown.md,
// --- RULED 2026-08-24; the book-wide anchor golden lives in
// --- markdown_golden.rs, the fence-rule mirror pins near the end of this
// --- section, and every §1.2 refusal has its own strict pin below) ----------

#[test]
fn markdown_parses_atx_headings_with_mdbook_ids() {
    // The §3 table's first two rows: `§`, the em-dash, `&` and `.` all drop,
    // each space becomes its own hyphen — measured against mdBook v0.5.4.
    assert_compiles_and_runs(
        r##"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	let source = "# Spec §1 — Introduction & conformance\n\n## 6.0 The law — owners, epochs, and claims\n";
        	match parse(source) {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::Heading(let level, let content, let id) => {
        						print("h" + level.to_string() + " " + id);
        					}
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "##,
        "h1 spec-1--introduction--conformance\nh2 60-the-law--owners-epochs-and-claims\n",
    );
}

#[test]
fn markdown_heading_ids_match_the_adversarial_corpus() {
    // The rest of the §3 corpus plus the shapes the LSP twin pins (impl:,
    // if/else with a closing run) and the non-ASCII cases — every id verified
    // against a local mdBook v0.5.4 build of this exact page.
    assert_compiles_and_runs(
        r####"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	let source = "## `Shared<T>`: one cell, many holders\n\n# Macros & const\n\n### Option::take and Option::replace\n\n## Conversions: `as_*`\n\n## `macro { … }` blocks\n\n## impl: methods and statics\n\n## if / else ##\n\n## Café naïveté\n\n## École Été\n\n## <a id=\"x\"></a> anchored\n";
        	match parse(source) {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::Heading(let level, let content, let id) => { print(id); }
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "####,
        "sharedt-one-cell-many-holders\nmacros--const\noptiontake-and-optionreplace\nconversions-as_\nmacro----blocks\nimpl-methods-and-statics\nif--else\ncafé-naïveté\nécole-été\nanchored\n",
    );
}

#[test]
fn markdown_dedupes_repeated_ids_in_document_order() {
    // §3 step 3: the second occurrence of a base becomes `-1`, the third
    // `-2` — and a heading inside a blockquote participates in document
    // order, exactly as a renderer meets it.
    assert_compiles_and_runs(
        r####"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun emit(blocks: List<Block>) {
        	for block in blocks {
        		match block {
        			Block::Heading(let level, let content, let id) => { print(id); }
        			Block::Quote(let inner) => { emit(inner); }
        			_ => {}
        		}
        	}
        }
        fun main() {
        	match parse("## Setup\n\n> ## Setup\n\n## Setup\n") {
        		Ok(let doc) => { emit(doc.blocks); }
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "####,
        "setup\nsetup-1\nsetup-2\n",
    );
}

#[test]
fn markdown_heading_id_is_the_dedupe_free_base() {
    // The public `heading_id` is §3's base algorithm: same input, same id,
    // no dedupe state between calls.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ heading_id, Inline };
        fun main() {
        	mut content: List<Inline> = [];
        	content.push(Inline::Text("Conversions: "));
        	content.push(Inline::Code("as_*"));
        	print(heading_id(content));
        	print(heading_id(content));
        }
        "#,
        "conversions-as_\nconversions-as_\n",
    );
}

#[test]
fn markdown_parses_inline_code_strong_emph_and_links() {
    // The inline constructs, with payload boundaries: span content
    // CommonMark-trimmed, `Link` is (destination, label), `_` emphasis only
    // at a word boundary (snake_case stays text).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun show(inlines: List<Inline>) {
        	for inline in inlines {
        		match inline {
        			Inline::Text(let t) => { print("text[" + t + "]"); }
        			Inline::Code(let t) => { print("code[" + t + "]"); }
        			Inline::Strong(let children) => { print("strong:"); show(children); }
        			Inline::Emph(let children) => { print("emph:"); show(children); }
        			Inline::Link(let dest, let label) => { print("link[" + dest + "]:"); show(label); }
        			Inline::Html(let raw) => { print("html[" + raw + "]"); }
        		}
        	}
        }
        fun main() {
        	match parse("a `code span` and **bold `x`** and *it* and _uh_ in snake_case [label](https://d) end\n") {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::Paragraph(let inlines) => { show(inlines); }
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "text[a ]\ncode[code span]\ntext[ and ]\nstrong:\ntext[bold ]\ncode[x]\ntext[ and ]\nemph:\ntext[it]\ntext[ and ]\nemph:\ntext[uh]\ntext[ in snake_case ]\nlink[https://d]:\ntext[label]\ntext[ end]\n",
    );
}

#[test]
fn markdown_parses_the_a_id_passthrough_and_autolink() {
    // The census's one HTML shape rides through verbatim; `<https://…>`
    // autolinks become links labeled with their destination.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	match parse("<a id=\"view\"></a>**view**: see <https://vilan-lang.org> now\n") {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::Paragraph(let inlines) => {
        						for inline in inlines {
        							match inline {
        								Inline::Html(let raw) => { print("html[" + raw + "]"); }
        								Inline::Link(let dest, let label) => { print("link[" + dest + "]"); }
        								_ => {}
        							}
        						}
        					}
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "html[<a id=\"view\">]\nhtml[</a>]\nlink[https://vilan-lang.org]\n",
    );
}

#[test]
fn markdown_parses_fenced_code_with_info_string_and_verbatim_body() {
    // The info string is carried verbatim (`vilan,browser` stays one
    // string); the body is byte-faithful with one trailing newline per line
    // — the docs gate's extraction shape — blank lines included.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	match parse("```vilan,browser\nlet x = 1;\n\n    deep();\n```\n") {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::CodeFence(let info, let body) => {
        						print("info[" + info + "]");
        						print("body[" + body + "]");
        					}
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "info[vilan,browser]\nbody[let x = 1;\n\n    deep();\n]\n",
    );
}

#[test]
fn markdown_parses_flat_lists_ordered_and_unordered() {
    // Census lists: `-` and `1.` markers, flat; a marker change starts a new
    // Items block; a simple item is one Paragraph of inlines.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	match parse("- alpha\n- beta\n  continued\n\n1. one\n2. two\n") {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::Items(let ordered, let items) => {
        						mut kind = "unordered";
        						if ordered { kind = "ordered"; }
        						print(kind + " " + items.len().to_string());
        					}
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "unordered 2\nordered 2\n",
    );
}

#[test]
fn markdown_a_list_item_carries_blocks() {
    // The recorded §2 deviation, pinned by the book's own shape
    // (tour/async.md): an item with a second paragraph and an indented
    // fence holds them as blocks — not flattened into siblings, not
    // glommed into the item's first line.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	let source = "- **Item.** first paragraph\n  wraps here\n\n  second paragraph\n\n  ```vilan\n  let x = 1;\n  ```\n\n- next item\n";
        	match parse(source) {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::Items(let ordered, let items) => {
        						print("items " + items.len().to_string());
        						for inner in items[0] {
        							match inner {
        								Block::Paragraph(let inlines) => { print("paragraph"); }
        								Block::CodeFence(let info, let body) => { print("fence[" + body + "]"); }
        								_ => { print("unexpected"); }
        							}
        						}
        					}
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "items 2\nparagraph\nparagraph\nfence[let x = 1;\n]\n",
    );
}

#[test]
fn markdown_parses_blockquotes_recursively() {
    // §2's probed recursion: a quote holds blocks, including another quote.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun depth(blocks: List<Block>, level: i32) {
        	for block in blocks {
        		match block {
        			Block::Quote(let inner) => {
        				print("quote@" + level.to_string());
        				depth(inner, level + 1);
        			}
        			Block::Paragraph(let inlines) => { print("paragraph@" + level.to_string()); }
        			_ => {}
        		}
        	}
        }
        fun main() {
        	match parse("> outer text\n>\n> > inner text\n") {
        		Ok(let doc) => { depth(doc.blocks, 0); }
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "quote@0\nparagraph@1\nquote@1\nparagraph@2\n",
    );
}

#[test]
fn markdown_parses_pipe_tables_and_unescapes_cell_pipes() {
    // Census tables: header + rows, no alignment — and the `\|` cell escape
    // (vilan's closure syntax in cells) unescapes before inline parsing, so
    // the code span carries a real `|`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	match parse("| op | means |\n|----|-------|\n| `\\|n\\| n * 2` | doubler |\n") {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::Table(let header, let rows) => {
        						print("header " + header.len().to_string() + " rows " + rows.len().to_string());
        						for inline in rows[0][0] {
        							match inline {
        								Inline::Code(let t) => { print("code[" + t + "]"); }
        								_ => { print("unexpected"); }
        							}
        						}
        					}
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "header 2 rows 1\ncode[|n| n * 2]\n",
    );
}

// --- std::markdown × the docs gate's fence rules (docs.rs extract_pins,
// --- mirrored — the package's fences must agree with the gate's) ------------

#[test]
fn markdown_bullet_indented_fence_extracts_and_dedents() {
    // docs.rs `bullet_indented_fence_extracts_and_dedents`: a fence indented
    // two columns under a bullet is found, and its body loses the fence's
    // own indent.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun emit(blocks: List<Block>) {
        	for block in blocks {
        		match block {
        			Block::CodeFence(let info, let body) => { print("body[" + body + "]"); }
        			Block::Items(let ordered, let items) => {
        				for item in items { emit(item); }
        			}
        			_ => {}
        		}
        	}
        }
        fun main() {
        	match parse("- A bullet:\n\n  ```vilan\n  let x = 1;\n  ```\n\n- Next bullet\n") {
        		Ok(let doc) => { emit(doc.blocks); }
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "body[let x = 1;\n]\n",
    );
}

#[test]
fn markdown_deeper_fence_body_lines_keep_relative_indent() {
    // docs.rs `nested_deeper_body_lines_keep_relative_indent`: only the
    // fence's columns come off; deeper indentation survives.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	match parse("  ```vilan\n  fun main() {\n      let x = 1;\n  }\n  ```\n") {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::CodeFence(let info, let body) => { print("body[" + body + "]"); }
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "body[fun main() {\n    let x = 1;\n}\n]\n",
    );
}

#[test]
fn markdown_an_indented_fence_does_not_swallow_following_prose() {
    // docs.rs `an_indented_fence_does_not_swallow_following_prose` (the D3
    // bug): the indented fence closes at its own indent, the prose stays
    // prose, and the flush fence after it survives.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun emit(blocks: List<Block>) {
        	for block in blocks {
        		match block {
        			Block::CodeFence(let info, let body) => { print("fence[" + body + "]"); }
        			Block::Paragraph(let inlines) => { print("paragraph"); }
        			Block::Items(let ordered, let items) => {
        				print("items");
        				for item in items { emit(item); }
        			}
        			_ => {}
        		}
        	}
        }
        fun main() {
        	match parse("- Bullet:\n\n  ```vilan\n  fun first() {}\n  ```\n\nThis prose must stay prose.\n\n```vilan\nfun second() {}\n```\n") {
        		Ok(let doc) => { emit(doc.blocks); }
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "items\nparagraph\nfence[fun first() {}\n]\nparagraph\nfence[fun second() {}\n]\n",
    );
}

#[test]
fn markdown_a_fence_like_line_at_a_different_indent_does_not_close() {
    // docs.rs `a_fence_like_line_inside_the_body_at_a_different_indent_does_
    // not_close`: a ``` deeper than the opener is body, not the closer.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::markdown::{ parse, Block, Doc, Inline, ParseError };
        import std::result::Result::{ Err, Ok };
        fun main() {
        	match parse("  ```vilan\n  outer\n    ```\n  more outer\n  ```\n") {
        		Ok(let doc) => {
        			for block in doc.blocks {
        				match block {
        					Block::CodeFence(let info, let body) => { print("body[" + body + "]"); }
        					_ => { print("unexpected"); }
        				}
        			}
        		}
        		Err(let error) => { print(error.to_string()); }
        	}
        }
        "#,
        "body[outer\n  ```\nmore outer\n]\n",
    );
}

// --- std::markdown strict refusals — one pin per §1.2 construct (the ruled
// --- failure mode: a loud ParseError naming the construct and its line) -----

fn assert_markdown_refuses(source_literal: &str, expected_error: &str) {
    // Each refusal pin drives the same tiny program: parse the literal,
    // print the error (or a loud "parsed" if the refusal regressed).
    let program = format!(
        r#"
        import std::print;
        import std::markdown::{{ parse, Doc, ParseError }};
        import std::result::Result::{{ Err, Ok }};
        fun main() {{
        	match parse("{source_literal}") {{
        		Ok(let doc) => {{ print("parsed"); }}
        		Err(let error) => {{ print(error.to_string()); }}
        	}}
        }}
        "#
    );
    assert_compiles_and_runs(&program, &format!("{expected_error}\n"));
}

#[test]
fn markdown_refuses_a_setext_heading() {
    assert_markdown_refuses(
        "Title\\n=====\\n",
        "line 2: a setext heading underline or thematic break (---, ===, ***) — both outside the census grammar; headings are ATX (# Title)",
    );
}

#[test]
fn markdown_refuses_a_thematic_break() {
    assert_markdown_refuses(
        "before\\n\\n---\\n\\nafter\\n",
        "line 3: a setext heading underline or thematic break (---, ===, ***) — both outside the census grammar; headings are ATX (# Title)",
    );
}

#[test]
fn markdown_refuses_a_nested_list() {
    assert_markdown_refuses(
        "- outer\\n  - inner\\n",
        "line 2: a nested list item — the census grammar's lists are flat",
    );
}

#[test]
fn markdown_refuses_an_indented_list_item() {
    assert_markdown_refuses(
        "  - indented\\n",
        "line 1: an indented list item — the census grammar's lists are flat, at the left margin",
    );
}

#[test]
fn markdown_refuses_an_indented_code_block() {
    assert_markdown_refuses(
        "para\\n\\n    let x = 1;\\n",
        "line 3: an indented code block (four or more leading spaces) — the census grammar's only code blocks are backtick fences",
    );
}

#[test]
fn markdown_refuses_a_footnote() {
    assert_markdown_refuses(
        "some text[^1] here\\n",
        "line 1: a footnote ([^label]) — footnotes are outside the census grammar",
    );
}

#[test]
fn markdown_refuses_a_reference_link() {
    assert_markdown_refuses(
        "a [text][label] link\\n",
        "line 1: a reference-style link ([text][label]) — only inline [text](destination) links are in the census grammar",
    );
}

#[test]
fn markdown_refuses_a_reference_definition() {
    assert_markdown_refuses(
        "[label]: https://example.com\\n",
        "line 1: a reference link definition ([label]: destination) — only inline [text](destination) links are in the census grammar",
    );
}

#[test]
fn markdown_refuses_an_image() {
    assert_markdown_refuses(
        "an ![alt](img.png) image\\n",
        "line 1: an image (![alt](destination)) — images are outside the census grammar",
    );
}

#[test]
fn markdown_refuses_strikethrough() {
    assert_markdown_refuses(
        "some ~~gone~~ text\\n",
        "line 1: strikethrough (~~text~~) — outside the census grammar",
    );
}

#[test]
fn markdown_refuses_a_raw_html_tag() {
    assert_markdown_refuses(
        "a <br> break\\n",
        "line 1: a raw HTML tag (<br>) — the census grammar's only HTML passthrough is <a id=\"…\"></a>; wrap literal <…> text (a generic like List<T>) in backticks",
    );
}

#[test]
fn markdown_refuses_a_backslash_escape() {
    assert_markdown_refuses(
        "escaped \\\\* star\\n",
        "line 1: a backslash escape (\\*) — the census grammar's only escape is \\| inside a table cell",
    );
}

#[test]
fn markdown_refuses_a_hard_line_break() {
    assert_markdown_refuses(
        "line one  \\nline two\\n",
        "line 1: a hard line break (two trailing spaces) — outside the census grammar",
    );
}

#[test]
fn markdown_refuses_a_backslash_line_break() {
    assert_markdown_refuses(
        "line one\\\\\\nline two\\n",
        "line 1: a backslash line break — outside the census grammar",
    );
}

#[test]
fn markdown_refuses_a_tilde_fence() {
    assert_markdown_refuses(
        "~~~\\ncode\\n~~~\\n",
        "line 1: a tilde fence (~~~) — the census grammar's fences use backticks",
    );
}

#[test]
fn markdown_refuses_an_unclosed_fence() {
    assert_markdown_refuses(
        "```vilan\\nlet x = 1;\\n",
        "line 1: an unclosed code fence — no closing ``` at the opening fence's indent",
    );
}

#[test]
fn markdown_refuses_a_table_alignment_colon() {
    assert_markdown_refuses(
        "| a | b |\\n|:--|--:|\\n| 1 | 2 |\\n",
        "line 2: a table alignment colon (:---) — the census grammar's tables carry no alignment",
    );
}

#[test]
fn markdown_refuses_a_custom_heading_id() {
    assert_markdown_refuses(
        "# Title {#custom}\\n",
        "line 1: a custom heading id ({#…}) — census-grammar heading ids are computed, mdBook's algorithm",
    );
}

#[test]
fn markdown_refuses_a_lazy_blockquote_continuation() {
    assert_markdown_refuses(
        "> quoted\\nlazy line\\n",
        "line 2: a lazy blockquote continuation — prefix the line with > or separate it from the quote with a blank line",
    );
}

#[test]
fn markdown_refuses_a_lazy_list_continuation() {
    assert_markdown_refuses(
        "- item\\nlazy line\\n",
        "line 2: a lazy list continuation — indent the line under its item (two spaces) or separate it from the list with a blank line",
    );
}

// ---------------------------------------------------------------------------
// B136 — an `is` test in a loop CONDITION must read the subject as of THIS
// iteration. The transformer hoisted the subject temp (`const $a = subject;`)
// into the enclosing block, BEFORE the `while`, so a body reassignment never
// reached the condition: `proposal/markdown.md` §10.7's repro printed 3 where
// 1 is correct, and the unbounded form looped forever. `if` position was
// always fine — the hazard is exactly the re-evaluated condition position.
// The fix walks the condition into its own prelude and, when that prelude is
// non-empty, emits `while (true) { <prelude> if (!cond) break; <body> }`.
// ---------------------------------------------------------------------------

#[test]
fn b136_an_is_in_a_loop_condition_reads_the_current_subject() {
    // The §10.7 minimal repro, verbatim: the first iteration sets `found`,
    // so the second condition evaluation must see `Some` and stop at 1.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            mut found: Option<i32> = None;
            mut cursor = 0;
            for (found is None) && cursor < 3 {
                found = Some(cursor);
                cursor += 1;
            }
            print(cursor);
        }
        "#,
        "1\n",
    );
}

#[test]
fn b136_an_unbounded_is_condition_loop_terminates() {
    // §10.7's "with no bounding conjunct: infinite loop" — no second
    // conjunct caps the trip count, so ONLY a re-evaluated `is` can end the
    // loop. Before the fix this hung (the spike port's surfacing symptom);
    // the budget is generous because it only guards against the hang.
    assert_runs_within(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            mut found: Option<i32> = None;
            for found is None {
                found = Some(7);
            }
            print(1);
        }
        "#,
        "1\n",
        std::time::Duration::from_secs(10),
    );
}

#[test]
fn b136_a_jump_break_bounded_is_condition_loop_exits_on_reassignment() {
    // The infinite-loop shape a program would actually write, bounded by a
    // `jump break` safety valve: the reassignment must end the loop at 1;
    // the stale hoist rode the valve to 3.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            mut found: Option<i32> = None;
            mut cursor = 0;
            for found is None {
                if cursor >= 3 {
                    jump break;
                }
                found = Some(cursor);
                cursor += 1;
            }
            print(cursor);
        }
        "#,
        "1\n",
    );
}

#[test]
fn b136_nested_loops_each_reevaluate_their_is_condition() {
    // Each level owns a hoist; both were stale (the inner one refreshed only
    // per OUTER iteration). One inner pass and one outer pass is correct.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            mut outer: Option<i32> = None;
            mut n = 0;
            for outer is None && n < 10 {
                mut inner: Option<i32> = None;
                for inner is None && n < 10 {
                    inner = Some(1);
                    n += 1;
                }
                outer = Some(1);
            }
            print(n);
        }
        "#,
        "1\n",
    );
}

#[test]
fn b136_a_loop_condition_is_binding_reads_the_current_payload() {
    // A BINDING pattern in the condition: the capture (`let v`) reads the
    // subject temp's payload slot, so a stale temp froze `v` at 3 and the
    // countdown never reached `None` (hang). Re-evaluating the prelude
    // rebinds the capture each iteration: 3+2+1+0 = 6.
    assert_runs_within(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            mut next: Option<i32> = Some(3);
            mut sum = 0;
            for next is Some(let v) {
                sum += v;
                if v == 0 {
                    next = None;
                } else {
                    next = Some(v - 1);
                }
            }
            print(sum);
        }
        "#,
        "6\n",
        std::time::Duration::from_secs(10),
    );
}

#[test]
fn b136_two_is_tests_in_one_condition_both_reevaluate() {
    // Two subjects, two hoists in one condition — both must move inside.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            mut a: Option<i32> = None;
            mut b: Option<i32> = None;
            mut n = 0;
            for a is None && b is None && n < 10 {
                a = Some(1);
                b = Some(2);
                n += 1;
            }
            print(n);
        }
        "#,
        "1\n",
    );
}

#[test]
fn b136_a_result_is_condition_reevaluates() {
    // §10.9 asked the fix lane to pin a `Result` variant beside the repro:
    // same shape over `Err`, with a binding to boot.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::result::Result::{ Ok, Err, self };
        fun main() {
            mut state: Result<i32, str> = Err("pending");
            mut n = 0;
            for state is Err(let _reason) && n < 5 {
                state = Ok(n);
                n += 1;
            }
            print(n);
        }
        "#,
        "1\n",
    );
}

#[test]
fn b136_an_is_in_an_if_inside_a_loop_stays_fresh() {
    // Control: `if` position was never wrong (§10.7 — "the same expression
    // in an `if` is fine"), and the fix must not disturb it. The `if` sees
    // `None` only on the first pass, so `found` pins to 0.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            mut found: Option<i32> = None;
            mut n = 0;
            for n < 3 {
                if found is None {
                    found = Some(n);
                }
                n += 1;
            }
            match found {
                Some(let v) => print(v),
                None => print(-1),
            };
        }
        "#,
        "0\n",
    );
}

// --- `str::substring` refuses rather than clamping or swapping -----------
// The host's `substring` clamps a negative bound to 0 and SWAPS an inverted
// pair, so `s.substring(offset, -1)` quietly returns the PREFIX — the
// complement of the request. The rule is now `0 <= start <= end <= len`,
// refused otherwise: at compile time where the bounds are literals, at run
// time where they are computed, and identically under `const`.

#[test]
fn substring_with_a_negative_literal_start_is_a_compile_error() {
    assert_fails_spanning(
        r#"
        import std::print;
        fun main() { print("hello".substring(-1, 3)); }
        main();
        "#,
        "-1",
        "substring start -1 is negative",
    );
}

#[test]
fn substring_with_a_negative_literal_end_is_a_compile_error() {
    // The exact spelling the ban is named for.
    assert_fails_spanning(
        r#"
        import std::print;
        fun main() { print("hello, world".substring(7, -1)); }
        main();
        "#,
        "-1",
        "substring end -1 is negative",
    );
}

#[test]
fn substring_with_literal_bounds_inverted_is_a_compile_error() {
    // No negative in sight: `start > end` is the same silent reinterpretation,
    // which is why the rule is stated as one inequality rather than a sign check.
    assert_fails_spanning(
        r#"
        import std::print;
        fun main() { print("hello".substring(5, 2)); }
        main();
        "#,
        "2",
        "substring end 2 is before its start 5",
    );
}

#[test]
fn substring_past_the_end_of_a_string_literal_is_a_compile_error() {
    // `end > len` is refused too, not clamped: a caller who means "to the end"
    // writes `s.len()`.
    assert_fails_spanning(
        r#"
        import std::print;
        fun main() { print("hello".substring(0, 100)); }
        main();
        "#,
        "100",
        "substring end 100 is past the length 5 of this string",
    );
}

#[test]
fn substring_names_the_replacement_verbs_in_its_note() {
    assert_fails_noting(
        r#"
        import std::print;
        fun main() { print("hello".substring(2, -1)); }
        main();
        "#,
        "substring end -1 is negative",
        "-1",
        "strip_suffix",
    );
}

#[test]
fn substring_admits_its_boundary_ranges() {
    // The refusal is of the ranges OUTSIDE `0 <= start <= end <= len`, not of
    // the degenerate ones inside it: both empty ends and the whole string work.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let s = "hello";
            print(s.substring(0, 0).len());
            print(s.substring(s.len(), s.len()).len());
            print(s.substring(2, 2).len());
            print(s.substring(0, s.len()));
        }
        main();
        "#,
        "0\n0\n0\nhello\n",
    );
}

#[test]
fn substring_with_a_computed_negative_start_panics() {
    assert_run_panics(
        r#"
        import std::print;
        fun main() {
            let s = "hello";
            let start = 0 - 1;
            print(s.substring(start, 3));
        }
        main();
        "#,
        "substring out of range: the length is 5 but the range is -1..3",
    );
}

#[test]
fn substring_with_a_computed_negative_end_panics() {
    assert_run_panics(
        r#"
        import std::print;
        fun main() {
            let s = "hello, world";
            let offset = 7;
            let end = 0 - 1;
            print(s.substring(offset, end));
        }
        main();
        "#,
        "substring out of range: the length is 12 but the range is 7..-1",
    );
}

#[test]
fn substring_with_a_computed_inverted_range_panics() {
    assert_run_panics(
        r#"
        import std::print;
        fun main() {
            let s = "hello";
            let start = 4;
            let end = 2;
            print(s.substring(start, end));
        }
        main();
        "#,
        "substring out of range: the length is 5 but the range is 4..2",
    );
}

#[test]
fn substring_with_a_computed_end_past_the_length_panics() {
    assert_run_panics(
        r#"
        import std::print;
        fun main() {
            let s = "hello";
            let end = 100;
            print(s.substring(0, end));
        }
        main();
        "#,
        "substring out of range: the length is 5 but the range is 0..100",
    );
}

#[test]
fn substring_out_of_range_fails_const_evaluation() {
    // Const eval and the runtime must agree: this arm used to reproduce JS's
    // clamp-and-swap faithfully, which would have folded a wrong string into
    // the build instead of failing it.
    assert_fails_with(
        r#"
        import std::print;
        fun cut(text: str, start: i32, end: i32): str { text.substring(start, end) }
        fun main() { let bad = const cut("hello", 4, 2); print(bad); }
        main();
        "#,
        "substring out of range: the length is 5 but the range is 4..2",
    );
}

#[test]
fn strip_prefix_and_strip_suffix_cut_or_report_absence() {
    // `starts_with`/`ends_with` test; these two cut — the verbs a caller
    // reaching for `substring(offset, -1)` actually wanted.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            match "data: 42".strip_prefix("data: ") {
                Some(let body) => print(body),
                None => print("none"),
            }
            match "report.md".strip_suffix(".md") {
                Some(let stem) => print(stem),
                None => print("none"),
            }
            match "data: 42".strip_prefix("zz") {
                Some(let body) => print(body),
                None => print("prefix absent"),
            }
            match "report.md".strip_suffix(".zz") {
                Some(let stem) => print(stem),
                None => print("suffix absent"),
            }
        }
        main();
        "#,
        "42\nreport\nprefix absent\nsuffix absent\n",
    );
}

#[test]
fn stripping_a_whole_match_is_some_empty_not_none() {
    // Why these return `Option<str>` and not `str`: "absent" and "present but
    // empty" are different answers, and a bare `str` could not tell them apart.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ None, Some, self };
        fun main() {
            match "ab".strip_prefix("ab") {
                Some(let rest) => print(i"some:{rest}"),
                None => print("none"),
            }
            match "ab".strip_suffix("ab") {
                Some(let rest) => print(i"some:{rest}"),
                None => print("none"),
            }
        }
        main();
        "#,
        "some:\nsome:\n",
    );
}

// --- B139: the recorded return answer is the FUNCTION's, never a caller's ----
//
// `infer_function_returns` serves `inferred_return_types` to skip re-deriving a
// chain (B139's TIME half, pinned for cost in `tests/deep_nesting.rs`). That
// record is keyed by FUNCTION ALONE, while an answer is recorded whenever the
// inference that produced it was exact — including when it ran under a caller's
// substitution. So the map genuinely does collect caller-shaped answers: over
// this suite, 1 157 records are written under a non-empty substitution context,
// and one generic enum's slot receives six different instantiations in a single
// run. The `substitution_context.is_empty()` guard on the READ is the only
// thing standing between those records and the next caller, and nothing named
// it until this pin.

/// The number of recorded return answers served to an ask carrying a caller's
/// generic bindings while `source` compiles, read on the worker the compile runs
/// on — the probe is thread-local, like the analyzer's other counters.
fn records_served_under_substitution(source: &str) -> u64 {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let before = vilan_core::analyzer::return_records_served_under_substitution();
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(
                program.is_some() && errors.is_empty(),
                "the plant must analyze cleanly, got: {errors:#?}"
            );
            vilan_core::analyzer::return_records_served_under_substitution() - before
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// The shapes that put caller-shaped answers into the record in the first place,
/// each instantiated at TWO types so a shared slot would be read across
/// bindings: a generic impl method with an undeclared return, a static hung off
/// a trait (the `b102`/`i5` shape whose records this suite was measured on), and
/// a plain generic function.
const CALLER_SHAPED_RETURN_PLANT: &str = r#"
        import std::print;

        trait Producer<T> {
            fun produce(self): T;
        }

        struct Holder<T> {
            item: T,
        }

        impl Holder<type T> with Producer<T> {
            fun produce(self) {
                self.item
            }
        }

        impl Producer<type T> {
            fun of(item: T): Holder<T> {
                Holder { item }
            }
        }

        fun echo<T>(value: T) {
            value
        }

        fun main() {
            let ints = Producer::of(1);
            let strs = Producer::of("hi");
            print(ints.produce());
            print(strs.produce());
            print(echo(2));
            print(echo("bye"));
        }
        "#;

/// A recorded answer is NEVER served to an ask that carries a caller's generic
/// bindings — the correctness half of B139's memo.
///
/// Non-vacuous, and provably so: deleting `substitution_context.is_empty()` from
/// the read in `infer_function_returns` turns this red immediately (925 serves
/// across this suite, 4 of them answers that differ from the correct one).
///
/// This asserts the guard rather than a wrong-typed program on purpose. A
/// behavioural pin was attempted first and does not exist today: with the guard
/// deleted, all 2 596 inference tests, the docs gate, and the byte-identical
/// corpus are unchanged, because a wrongly-served answer is re-substituted
/// downstream before it can reach codegen. The hazard is real — the records are
/// caller-shaped — but only this guard makes it unreachable, so this is what
/// there is to pin.
#[test]
fn b139_a_recorded_return_is_never_served_under_a_callers_bindings() {
    assert_eq!(
        records_served_under_substitution(CALLER_SHAPED_RETURN_PLANT),
        0,
        "a recorded return answer was served to an ask carrying a caller's \
         generic bindings — the record is keyed by function alone, so that \
         answer belongs to a DIFFERENT caller (B139)"
    );
}

/// The plant is not vacuous either: it really does compile and run, and really
/// does instantiate each generic shape at two distinct types. A plant that
/// stopped exercising the shape would make the pin above pass for free.
#[test]
fn b139_the_caller_shaped_return_plant_runs_at_both_instantiations() {
    assert_compiles_and_runs(CALLER_SHAPED_RETURN_PLANT, "1\nhi\n2\nbye\n");
}

// --- `std::path` (kolt.local 017) --------------------------------------------
//
// The module is free functions over `str`, POSIX-shaped (`/` only, on every
// platform), and colorless — the three forks the item filed, settled in
// `vilan/std/src/path.vl`'s header. What follows is one pin per case rather
// than one per function: the edges that bite are trailing separators, `.` and
// `..`, the absolute/relative split between `join` and `resolve`, and the
// dotfile rule in `extname`, and each of those is where a hand-rolled version
// goes wrong.
//
// Answers were differentialled against node's `path.posix` over a 34-case
// table before they were pinned. They agree case for case with TWO deliberate
// divergences, both pinned below so they cannot drift back by accident:
// `normalize` drops a trailing separator where node keeps it, and
// `dirname("a//b")` is `"a"` where node leaves the dangling `"a/"`.

#[test]
fn path_normalize_collapses_separators_and_resolves_dot() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::normalize("a//b/./c"));
            print(path::normalize("a/b/../c"));
            print(path::normalize("./a"));
            print(path::normalize(""));
            print(path::normalize("."));
            print(path::normalize("/"));
            print(path::normalize("//"));
        }
        main();
        "#,
        "a/b/c\na/c\na\n.\n.\n/\n/\n",
    );
}

#[test]
fn path_normalize_drops_a_trailing_separator_where_node_keeps_it() {
    // The one divergence from `path.posix.normalize`, and the point of the
    // function: two spellings of one path must compare equal, and node's
    // "a/b" / "a/b/" do not — which is how a cache or an asset map ends up
    // with two entries for one file.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::normalize("a/b/"));
            print(path::normalize("/a/b/"));
            print(path::normalize("a/"));
            print(path::normalize("/"));
            print(path::normalize("a/b") == path::normalize("a/b/"));
        }
        main();
        "#,
        "a/b\n/a/b\na\n/\ntrue\n",
    );
}

#[test]
fn path_normalize_stops_at_an_absolute_root_but_keeps_a_relative_climb() {
    // A `..` above the root is dropped (the root's parent is the root); a
    // leading `..` on a relative path is kept, because it names something.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::normalize("/../a"));
            print(path::normalize("/a/../../.."));
            print(path::normalize("../a"));
            print(path::normalize("a/../../b"));
            print(path::normalize(".."));
        }
        main();
        "#,
        "/a\n/\n../a\n../b\n..\n",
    );
}

#[test]
fn path_functions_never_fold_case() {
    // `windows-support.md` §5 enforces case-EXACT module resolution precisely
    // so that `import foo` cannot resolve `Foo.vl` on NTFS. A path module that
    // decided `Foo` and `foo` were the same path would hand that back, so
    // every comparison here is byte-for-byte and `normalize` preserves the
    // case it was given.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        import std::option::Option::{ None, Some, self };
        fun main() {
            print(path::normalize("Foo/../BAR"));
            print(path::basename("/a/README.md"));
            print(path::starts_with("/A/b", "/a"));
            match path::relative("/A", "/a") {
                Some(let answer) => print(answer),
                None => print("none"),
            }
        }
        main();
        "#,
        "BAR\nREADME.md\nfalse\n../a\n",
    );
}

#[test]
fn path_join_does_not_reset_on_an_absolute_second_argument_but_resolve_does() {
    // The split that decides what each verb is for. `join` is textual; the
    // caller asking "…unless the second is already absolute" is asking about
    // REFERENCES and wants `resolve`. A `join` that silently discarded its
    // first argument would be a traversal primitive wearing a concatenation's
    // name.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::join("/a", "/b"));
            print(path::resolve("/a", "/b"));
            print(path::join("/a", "b"));
            print(path::resolve("/a", "b"));
        }
        main();
        "#,
        "/a/b\n/b\n/a/b\n/a/b\n",
    );
}

#[test]
fn path_join_treats_an_empty_side_as_nothing() {
    // `join("", "b")` must not become absolute — the empty base contributes
    // no separator, it contributes nothing at all.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::join("", "b"));
            print(path::join("a", ""));
            print(path::join("", ""));
            print(path::join("a", "../b"));
            print(path::join("a", ".."));
        }
        main();
        "#,
        "b\na\n.\nb\n.\n",
    );
}

#[test]
fn path_join_all_folds_left_and_answers_the_empty_list() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            let parts: List<str> = ["a", "b", "c"];
            print(path::join_all(parts));
            let nothing: List<str> = [];
            print(path::join_all(nothing));
            let climbing: List<str> = ["/a", "..", "b"];
            print(path::join_all(climbing));
        }
        main();
        "#,
        "a/b/c\n.\n/b\n",
    );
}

#[test]
fn path_basename_ignores_trailing_separators_and_the_root_has_none() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::basename("/a/b.txt"));
            print(path::basename("/a/b/"));
            print(path::basename("a"));
            print(path::basename("/"));
            print(path::basename(""));
            print(path::basename("a/.."));
        }
        main();
        "#,
        "b.txt\nb\na\n\n\n..\n",
    );
}

#[test]
fn path_dirname_stops_at_the_root_and_answers_dot_without_a_separator() {
    // `dirname("a//b")` is the second divergence from node, which answers
    // `"a/"` — a parent path carrying a dangling separator, which then has to
    // be normalized away by whoever receives it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::dirname("/a/b.txt"));
            print(path::dirname("/a"));
            print(path::dirname("/"));
            print(path::dirname("a"));
            print(path::dirname(""));
            print(path::dirname("a/b/"));
            print(path::dirname("a//b"));
            print(path::dirname("/../a"));
        }
        main();
        "#,
        "/a\n/\n/\n.\n.\na\na\n/..\n",
    );
}

#[test]
fn path_extname_gives_a_dotfile_no_extension() {
    // The landmine this module exists to defuse. `.gitignore` is a hidden
    // file, not a file of type "gitignore": the leading dot marks it, it does
    // not name a type. `.` and `..` likewise have none. (node's
    // `path.extname` answers the same way and hand-rolled versions rarely do.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::extname(".gitignore"));
            print(path::extname("a/.gitignore"));
            print(path::extname("."));
            print(path::extname(".."));
            print(path::extname(".a.b"));
        }
        main();
        "#,
        "\n\n\n\n.b\n",
    );
}

#[test]
fn path_extname_reads_the_last_dot_of_the_last_component() {
    // A dot in a DIRECTORY name is invisible from here — the question is
    // about the file — and a trailing dot is an empty extension spelled ".".
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::extname("index.html"));
            print(path::extname("a.b.c"));
            print(path::extname("noext"));
            print(path::extname("file."));
            print(path::extname("a.b/c"));
            print(path::extname("/a/b/"));
        }
        main();
        "#,
        ".html\n.c\n\n.\n\n\n",
    );
}

#[test]
fn path_stem_and_extname_cut_the_basename_in_two() {
    // `stem(p) + extname(p) == basename(p)` for every p, including the
    // degenerate spellings — the invariant that makes the pair safe to use
    // together instead of re-deriving the split at each call site.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun holds(candidate: str): bool {
            path::stem(candidate) + path::extname(candidate) == path::basename(candidate)
        }
        fun main() {
            print(path::stem("/a/b.txt"));
            print(path::stem(".gitignore"));
            print(path::stem("file."));
            print(path::stem("a/b/"));
            print(holds("/a/b.txt"));
            print(holds(".gitignore"));
            print(holds("file."));
            print(holds("..."));
            print(holds("/"));
        }
        main();
        "#,
        "b\n.gitignore\nfile\nb\ntrue\ntrue\ntrue\ntrue\ntrue\n",
    );
}

#[test]
fn path_starts_with_compares_components_where_the_str_verb_compares_text() {
    // The answer to "do `str::strip_prefix`/`starts_with` suffice for paths":
    // they do not, and the first two lines are the proof. `/a/bc` starts with
    // the TEXT `/a/b` and is not inside the DIRECTORY `/a/b`. Deriving an
    // asset key by cutting a textual prefix is the same mistake, which is what
    // kolt.local 023 filed.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print("/a/bc".starts_with("/a/b"));
            print(path::starts_with("/a/bc", "/a/b"));
            print(path::starts_with("/a/b/c", "/a/b"));
            print(path::starts_with("/a/b", "/a/b"));
            print(path::starts_with("/a/b", "/a/"));
            print(path::starts_with("/a/b", "/"));
            print(path::starts_with("a/b", "/a"));
            print(path::starts_with("/a", "/a/b"));
        }
        main();
        "#,
        "true\nfalse\ntrue\ntrue\ntrue\ntrue\nfalse\nfalse\n",
    );
}

#[test]
fn path_relative_inverts_resolve() {
    // `resolve(from, relative(from, to))` is `normalize(to)` — the property
    // that makes the pair usable for rebasing a whole tree of paths.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        import std::option::Option::{ None, Some, self };
        fun step(from: str, to: str) {
            match path::relative(from, to) {
                Some(let hop) => print(path::resolve(from, hop) == path::normalize(to)),
                None => print("none"),
            }
        }
        fun main() {
            print(path::relative("/a/b", "/a/c").unwrap_or("none"));
            print(path::relative("/a/b", "/a/b/c").unwrap_or("none"));
            print(path::relative("/a/b", "/a/b").unwrap_or("none"));
            print(path::relative("/", "/a").unwrap_or("none"));
            print(path::relative("/a", "/").unwrap_or("none"));
            step("/a/b", "/x/y");
            step("a/b", "a/b/c/d");
            step("/a/b/", "/a/b/c/");
        }
        main();
        "#,
        "../c\nc\n.\na\n..\ntrue\ntrue\ntrue\n",
    );
}

#[test]
fn path_relative_is_none_where_there_is_no_lexical_answer() {
    // Two cases, both real: the two sides disagree about being absolute (no
    // working directory here to bridge them), or `from` still begins with `..`
    // after the common prefix comes off (climbing out of an unknown place, so
    // what is above it is unknown too). An `Option` for `strip_prefix`'s
    // reason — "no answer" and "the answer is `.`" are different.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        import std::option::Option::{ None, Some, self };
        fun show(from: str, to: str) {
            match path::relative(from, to) {
                Some(let hop) => print(hop),
                None => print("none"),
            }
        }
        fun main() {
            show("/a/b", "b");
            show("b", "/a/b");
            show("../a", "b");
            show("a", "../b");
        }
        main();
        "#,
        "none\nnone\nnone\n../../b\n",
    );
}

#[test]
fn std_path_is_colorless_and_serves_a_browser_build() {
    // The coloring call, verified rather than assumed. `std::fs` and friends
    // are seeded `@process` by living in `src/process` (`std/vilan.toml`'s
    // `[library.layer.process]`), and a browser build that reaches one is
    // refused. `std::path` is in the base `root` layer and has no host call
    // in it, so it serves both — which is the point: a browser router and an
    // SSR render manipulate the same `/`-separated strings, and a
    // `@process`-colored path module would have put that shared half out of
    // reach of half the program.
    assert_compiles_browser(
        r#"
        import std::print;
        import std::path;
        fun main() {
            print(path::join("/assets", "app.css"));
            print(path::basename("/assets/app.css"));
        }
        "#,
    );
}

#[test]
fn path_arithmetic_folds_under_const() {
    // Pure vilan with no host call, so it is const-evaluable — the property a
    // build-time consumer (bundled-asset paths, kolt.local 029) needs, and one
    // a `node:path` binding could never have had.
    let js = compile(
        r#"
        import std::print;
        import std::path;
        fun main() {
            let folded = const path::join("dist", "../dist/app.js");
            print(folded);
        }
        main();
        "#,
    )
    .expect("expected a clean compile");
    assert!(
        js.contains("const folded = \"dist/app.js\";"),
        "expected the join to fold to a literal at compile time, got:\n{js}"
    );
}

#[test]
fn path_strip_prefix_cuts_only_where_starts_with_agrees() {
    // The path sibling of `str::strip_prefix` (Order 12), and the last line is
    // why it had to exist: the `str` verb answers `Some("c")` for a path that
    // is not inside the prefix at all. `starts_with` tests, this one cuts —
    // and it is a different question from `relative`, which always has an
    // answer for two paths under one root and will climb with `..` to reach
    // it. Cutting the whole of a path gives `Some(".")`, matching `relative`'s
    // answer for a path to itself.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::path;
        import std::option::Option::{ None, Some, self };
        fun show(path: str, prefix: str) {
            match path::strip_prefix(path, prefix) {
                Some(let rest) => print(rest),
                None => print("none"),
            }
        }
        fun main() {
            show("/srv/site/css/app.css", "/srv/site");
            show("/a/bc", "/a/b");
            show("/a/b", "/a/b");
            show("/a/b", "/a/b/");
            show("/a/b", "/");
            show("a/b/c", "a");
            show("a/b", "/a");
            show("/A/b", "/a");
            print("/a/bc".strip_prefix("/a/b").unwrap_or("?"));
        }
        main();
        "#,
        "css/app.css\nnone\n.\n.\na/b\nb/c\nnone\nnone\nc\n",
    );
}

// --- kolt.local 029: the const output channel for FILES — `asset::bundle` ------
// `emit`'s sibling on the other axis. `emit` accumulates LINES into one
// generated file; `bundle` carries an EXISTING file through unchanged, so a
// built app needs nothing but `dist/`. Same const-only bit, same package-root
// resolution, same build-input record — the three properties `asset::read`
// already had, now pointing the other way.

/// A clean analysis with an explicit package root, returning the folded const
/// values, the recorded build inputs, and the files registered for bundling.
fn const_bundles(
    source: &str,
    root: &Path,
) -> (
    Vec<vilan_core::interpreter::ConstValue>,
    Vec<(PathBuf, Option<u64>)>,
    Vec<(PathBuf, String)>,
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
                program.const_bundled_files.clone(),
            )
        })
        .unwrap()
        .join()
        .unwrap()
}

/// The book's own tree, used as a package root: the pins below bundle files
/// that really exist rather than staging a fixture for each one.
fn bundle_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan")
}

#[test]
fn a_const_bundle_registers_the_file_and_folds_to_its_url() {
    let (values, inputs, bundled) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle("docs/SUMMARY.md");
        }
        main();
        "#,
        &bundle_root(),
    );
    assert_eq!(
        values,
        vec![vilan_core::interpreter::ConstValue::Str(
            "/docs/SUMMARY.md".to_string()
        )],
        "the call folds to the url its bundled copy answers on"
    );
    assert_eq!(bundled.len(), 1, "one registered file: {bundled:?}");
    assert_eq!(
        bundled[0].1, "docs/SUMMARY.md",
        "the path IS the name — the subdirectory survives: {bundled:?}"
    );
    assert!(
        bundled[0].0.ends_with("docs/SUMMARY.md"),
        "resolved against the package root: {bundled:?}"
    );
    assert_eq!(inputs.len(), 1, "one tracked input: {inputs:?}");
    assert!(
        inputs[0].1.is_some(),
        "a bundled file is a HASHED build input, exactly as a read one is — \
         that record is what makes an edited resource drive a watch round: \
         {inputs:?}"
    );
}

#[test]
fn a_file_bundled_twice_is_registered_once() {
    // Two call sites, one file: the copy is idempotent, so the registry must
    // not name it twice (a duplicate would put it in the manifest twice and
    // make `serve_build` install two identical routes).
    let (_, _, bundled) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _one = const asset::bundle("docs/SUMMARY.md");
            let _two = const asset::bundle("./docs/SUMMARY.md");
        }
        main();
        "#,
        &bundle_root(),
    );
    assert_eq!(
        bundled.len(),
        1,
        "one file, one registration — and `./` normalizes to the same name: {bundled:?}"
    );
    assert_eq!(bundled[0].1, "docs/SUMMARY.md");
}

#[test]
fn a_missing_bundle_is_a_clean_diagnostic_at_the_call_site() {
    assert_fails_spanning(
        r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle("vilan-029-definitely-missing.png");
        }
        main();
        "#,
        r#"asset::bundle("vilan-029-definitely-missing.png")"#,
        "cannot bundle `vilan-029-definitely-missing.png`",
    );
}

#[test]
fn a_missing_bundle_is_still_a_tracked_build_input() {
    // A file that was not there is still a dependency: its APPEARANCE must
    // invalidate the compile that failed on it, exactly as a change to a
    // present one does. The analysis fails, so the record is read off the
    // program the failing analysis left rather than through `const_bundles`.
    let root = bundle_root();
    let source = r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle("vilan-029-definitely-missing.png");
        }
        main();
        "#
    .to_string();
    let inputs = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, _errors) = analyze_source(
                leaked,
                &std_spec(),
                &root,
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            program
                .map(|program| program.const_input_files.clone())
                .unwrap_or_default()
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(
        inputs.iter().any(
            |(path, hash)| path.ends_with("vilan-029-definitely-missing.png") && hash.is_none()
        ),
        "the miss must be recorded, unhashed: {inputs:?}"
    );
}

#[test]
fn an_absolute_bundle_path_is_refused() {
    // Per-platform for the reason `an_absolute_read_path_is_refused` records
    // (N26). This pin was modelled on that one and inherited its defect with
    // it, which is how one bad pin became two.
    //
    // The Windows spelling is FORWARD-slashed, and that is not cosmetic:
    // `bundled_name` refuses a backslash BEFORE it tests for absolute, because
    // a bundled name is `/`-separated on every host and a backslash is refused
    // rather than translated. So a Windows-NATIVE `C:\...` path never reaches
    // the arm this pin is named for - it is caught one check earlier, which is
    // exactly what happened when this pin was first repaired and Windows CI
    // stayed red on it. `C:/...` is absolute to Windows all the same (a prefix
    // plus a root), so it reaches the arm. The backslash check has its own pin,
    // `a_backslash_in_a_bundle_path_is_refused`, and this one must not
    // accidentally re-test it.
    let absolute = if cfg!(windows) {
        "C:/Windows/system.ini"
    } else {
        "/etc/hostname"
    };
    assert_fails_with(
        &format!(
            r#"
        import std::asset;
        fun main() {{
            let _url = const asset::bundle("{absolute}");
        }}
        main();
        "#
        ),
        &format!(
            "`asset::bundle` paths are relative to the package root; `{absolute}` is absolute"
        ),
    );
}

#[test]
fn a_bundle_path_escaping_the_package_root_is_refused() {
    assert_fails_with(
        r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle("../outside.png");
        }
        main();
        "#,
        "`asset::bundle` paths resolve inside the package root; `../outside.png` escapes it",
    );
}

#[test]
fn a_backslash_in_a_bundle_path_is_refused() {
    // POSIX-only, for the reason `std::path` is (kolt.local 017): the name is
    // derived OUTPUT — a url, a manifest row, a golden — and a separator-aware
    // rule would make every one of them host-dependent. `\` is refused rather
    // than translated, so a path that means two things on two hosts means
    // nothing on either.
    assert_fails_with(
        r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle("static\\logo.png");
        }
        main();
        "#,
        "`asset::bundle` paths are `/`-separated on every host",
    );
}

#[test]
fn a_bundle_path_naming_no_file_is_refused() {
    // `"."` and `""` resolve to the package root itself, which is a directory
    // and not a resource. Refused by name rather than by the read failing, so
    // the message says what is wrong instead of reporting an OS error.
    assert_fails_with(
        r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle(".");
        }
        main();
        "#,
        "`asset::bundle` needs a file inside the package root; `.` names none",
    );
}

#[test]
fn a_runtime_bundle_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::asset;
        fun main() {
            let _url = asset::bundle("logo.png");
        }
        main();
        "#,
        r#"asset::bundle("logo.png")"#,
        "compile-time-only",
    );
}

#[test]
fn a_runtime_call_reaching_bundle_is_rejected_at_the_boundary() {
    // The R-fixpoint names WHICH builtin the path reaches — a bundle-reaching
    // function says `asset::bundle`, not `asset::emit`.
    assert_fails_with(
        r#"
        import std::asset;
        fun icon(): str {
            asset::bundle("logo.png")
        }
        fun main() {
            let _url = icon();
        }
        main();
        "#,
        "`icon` (it reaches `asset::bundle`) is compile-time-only",
    );
}

#[test]
fn a_function_reaching_bundle_cannot_escape_as_a_value() {
    assert_fails_with(
        r#"
        import std::asset;
        fun icon(): str {
            asset::bundle("logo.png")
        }
        fun apply(f: || str): str {
            f()
        }
        fun main() {
            let _url = apply(icon);
        }
        main();
        "#,
        "no runtime value form",
    );
}

#[test]
fn a_changed_bundled_file_is_seen_by_the_next_analysis() {
    // The invalidation pin, `asset::read`'s sibling: analyze, EDIT THE FILE,
    // analyze again in the same process — the second analysis must record the
    // new hash. If any cache ever keys const results without the bundled
    // inputs, a `--watch` round stops recopying an edited resource and the dev
    // loop serves last round's bytes forever.
    let dir = std::env::temp_dir().join(format!("vilan-const-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("static")).unwrap();
    let source = r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle("static/note.txt");
        }
        main();
        "#;
    std::fs::write(dir.join("static/note.txt"), "one").unwrap();
    let (values, first, bundled) = const_bundles(source, &dir);
    assert_eq!(
        values,
        vec![vilan_core::interpreter::ConstValue::Str(
            "/static/note.txt".to_string()
        )]
    );
    assert_eq!(bundled.len(), 1);
    std::fs::write(dir.join("static/note.txt"), "two").unwrap();
    let (_, second, _) = const_bundles(source, &dir);
    assert_ne!(
        first[0].1, second[0].1,
        "the edited resource must re-hash to a different input record — a \
         stale hash is a resource that stops being recopied: {first:?} {second:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bundled_file_is_not_charged_by_its_size() {
    // Deliberately unlike `asset::read`, whose bytes become a `str` the const
    // program then computes over: a bundled file's bytes never enter the
    // program, so charging fuel by size would bound how large an asset may be
    // rather than how much work a build does. A file comfortably past the
    // explicit fuel budget in bytes bundles fine.
    let dir = std::env::temp_dir().join(format!("vilan-const-bundle-fuel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("huge.bin"), "a".repeat(17_000_000)).unwrap();
    let (values, _, bundled) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle("huge.bin");
        }
        main();
        "#,
        &dir,
    );
    assert_eq!(
        values,
        vec![vilan_core::interpreter::ConstValue::Str(
            "/huge.bin".to_string()
        )],
        "a 17 MB resource is a build output, not a compile-time computation"
    );
    assert_eq!(bundled.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// --- kolt.local 035: the estate verbs — `read_dir`/`read_dir_all`, `bundle_as`,
// --- `digest` (const-eval.md §3.1) ---------------------------------------------
// The three gaps 029 left. `read_dir`/`read_dir_all` let const code ENUMERATE a
// directory, so a static estate is a loop instead of a hand-written list of
// `bundle` calls; `bundle_as` spells the target AT THE CALL, so a file can
// answer on a url its path does not spell without anything being renamed behind
// the author's back; `digest` is the sha-256 that makes a content-hashed url
// mintable in the language that ships the file.

/// A staged package root for the listing pins: a small tree with a nested
/// directory, written in an order the sort has to undo, so a pin asserting the
/// listing's ORDER is asserting the sort and not the host's `readdir`.
fn estate_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vilan-035-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("static/icons")).expect("create the estate");
    // Deliberately reverse order, and the nested file first.
    std::fs::write(root.join("static/icons/open.svg"), "open").expect("write");
    std::fs::write(root.join("static/icons/close.svg"), "close").expect("write");
    std::fs::write(root.join("static/robots.txt"), "ROBOTS\n").expect("write");
    std::fs::write(root.join("static/logo.png"), "abc").expect("write");
    root
}

/// The diagnostics an analysis against `root` produced — the failing-path twin
/// of [`const_bundles`], which asserts a clean one.
fn const_errors(source: &str, root: &Path) -> Vec<String> {
    let source = source.to_string();
    let root = root.to_path_buf();
    std::thread::Builder::new()
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
            errors.into_iter().map(|error| error.msg).collect()
        })
        .unwrap()
        .join()
        .unwrap()
}

#[track_caller]
fn assert_named(errors: &[String], part: &str) {
    assert!(
        errors.iter().any(|error| error.contains(part)),
        "no diagnostic contains {part:?}; got: {errors:#?}"
    );
}

/// The list a `const` folded to, as plain strings.
#[track_caller]
fn strings(value: &vilan_core::interpreter::ConstValue) -> Vec<String> {
    match value {
        vilan_core::interpreter::ConstValue::Array(elements) => elements
            .iter()
            .map(|element| match element {
                vilan_core::interpreter::ConstValue::Str(text) => text.clone(),
                other => panic!("a listing must fold to strings, got {other:?}"),
            })
            .collect(),
        other => panic!("a listing must fold to a list, got {other:?}"),
    }
}

#[test]
fn a_recursive_listing_is_byte_sorted_over_files_only() {
    // BOTH divergences from `std::fs::read_dir_all`, in one assert, because
    // either one alone would pass vacuously on the other's failure:
    //
    //   - **Sorted.** A const result is compiled INTO the output, so host
    //     iteration order would make one source tree produce two builds. This
    //     is the determinism §9.5 rests on, not a convenience.
    //   - **Files only.** `icons` is a directory and is NOT an entry: nothing
    //     in the channel consumes a directory and there is no const `stat`, so
    //     a directory entry would be one the estate recipe below could neither
    //     act on nor filter out.
    let root = estate_root("listing");
    let (values, _, _) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _entries = const asset::read_dir_all("static");
        }
        main();
        "#,
        &root,
    );
    assert_eq!(
        strings(&values[0]),
        vec![
            "icons/close.svg",
            "icons/open.svg",
            "logo.png",
            "robots.txt"
        ],
        "byte-sorted on the WHOLE relative path, files only — `icons` itself is \
         not an entry"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_immediate_listing_names_bare_entries_and_stops_there() {
    // `read_dir`'s half of the shape `std::fs` sets: IMMEDIATE entries, as bare
    // names rather than joined paths (what addresses one is `i"{dir}/{name}"`),
    // and no descent — `icons/close.svg` is `read_dir_all`'s answer, not this
    // one's.
    let root = estate_root("immediate");
    let (values, _, _) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _entries = const asset::read_dir("static");
        }
        main();
        "#,
        &root,
    );
    assert_eq!(
        strings(&values[0]),
        vec!["logo.png", "robots.txt"],
        "immediate FILES, bare names, sorted — no descent and no directory"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_listed_directory_is_a_tracked_build_input() {
    // The recorded-inputs doctrine `read` uses, per DIRECTORY WALKED — which is
    // what makes it compose: a file appearing or disappearing anywhere in the
    // tree moves exactly one recorded directory's key, so both directions
    // invalidate. The keys must be the ones `directory_input_hash` computes,
    // because that is what the CLI's per-leg skip re-hashes them with; if the
    // two ever disagree, a listed directory disqualifies every skip forever (or,
    // worse, compares equal when it should not).
    let root = estate_root("tracked");
    let (_, inputs, _) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _entries = const asset::read_dir_all("static");
        }
        main();
        "#,
        &root,
    );
    for directory in ["static", "static/icons"] {
        let path = root.join(directory);
        let recorded = inputs
            .iter()
            .find(|(recorded, _)| *recorded == path)
            .unwrap_or_else(|| panic!("`{directory}` was not recorded: {inputs:?}"));
        assert_eq!(
            recorded.1,
            vilan_core::const_eval::directory_input_hash(&path),
            "the const pass and the per-leg skip must key `{directory}` the same way"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_file_moves_the_listed_directorys_input_key() {
    // Appearance and disappearance both. Without this the estate recipe is a
    // one-shot: a resource added under `--watch` would never join the build,
    // because the leg's inputs would re-hash equal and the leg would skip.
    let root = estate_root("membership");
    let source = r#"
        import std::asset;
        fun main() {
            let _entries = const asset::read_dir_all("static");
        }
        main();
        "#;
    let (_, before, _) = const_bundles(source, &root);
    std::fs::write(root.join("static/icons/menu.svg"), "menu").expect("add a file");
    let (values, after, _) = const_bundles(source, &root);
    let key = |inputs: &[(PathBuf, Option<u64>)]| {
        inputs
            .iter()
            .find(|(path, _)| *path == root.join("static/icons"))
            .expect("the nested directory is recorded")
            .1
    };
    assert_ne!(
        key(&before),
        key(&after),
        "a file appearing in a listed directory must move its input key"
    );
    assert!(
        strings(&values[0]).contains(&"icons/menu.svg".to_string()),
        "and the new file must be in the listing: {:?}",
        strings(&values[0])
    );
    std::fs::remove_file(root.join("static/icons/menu.svg")).expect("remove it again");
    let (_, restored, _) = const_bundles(source, &root);
    assert_eq!(
        key(&before),
        key(&restored),
        "and removing it must put the key back — disappearance is the same event"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_listing_that_names_no_directory_is_refused() {
    // A missing path and a path that is a FILE are both refused at the `const`
    // expression, and the message names the call the author wrote. `read`'s
    // posture, and `std::fs`'s.
    let root = estate_root("nodir");
    for (path, function) in [
        ("nowhere", "asset::read_dir"),
        ("static/logo.png", "asset::read_dir"),
    ] {
        let errors = const_errors(
            Box::leak(
                format!(
                    r#"
        import std::asset;
        fun main() {{
            let _entries = const asset::read_dir("{path}");
        }}
        main();
        "#
                )
                .into_boxed_str(),
            ),
            &root,
        );
        assert_named(&errors, &format!("cannot list `{path}` with `{function}`"));
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_listing_path_outside_the_package_root_is_refused() {
    // `read`'s fence, lexically, before any filesystem look — and deliberately
    // NOT `bundle`'s, which also refuses a backslash: a listing's argument
    // addresses a directory to READ and never becomes derived output.
    let absolute = if cfg!(windows) { "C:/Windows" } else { "/etc" };
    assert_fails_with(
        Box::leak(
            format!(
                r#"
        import std::asset;
        fun main() {{
            let _entries = const asset::read_dir("{absolute}");
        }}
        main();
        "#
            )
            .into_boxed_str(),
        ),
        &format!(
            "`asset::read_dir` paths are relative to the package root; `{absolute}` is absolute"
        ),
    );
    assert_fails_with(
        r#"
        import std::asset;
        fun main() {
            let _entries = const asset::read_dir_all("../outside");
        }
        main();
        "#,
        "`asset::read_dir_all` paths resolve inside the package root; `../outside` escapes it",
    );
}

#[test]
fn a_runtime_listing_is_rejected() {
    // Const-only exactly like its siblings, and the R-fixpoint names WHICH verb
    // the path reaches — a listing-reaching function says `asset::read_dir_all`,
    // not `asset::emit`.
    assert_fails_spanning(
        r#"
        import std::asset;
        fun main() {
            let _entries = asset::read_dir("static");
        }
        main();
        "#,
        r#"asset::read_dir("static")"#,
        "compile-time-only",
    );
    assert_fails_with(
        r#"
        import std::asset;
        fun estate(): List<str> {
            asset::read_dir_all("static")
        }
        fun main() {
            let _entries = estate();
        }
        main();
        "#,
        "`estate` (it reaches `asset::read_dir_all`) is compile-time-only",
    );
}

#[test]
fn a_bundle_as_target_is_the_url_and_the_output_name() {
    // The whole point: the file stays where the author put it, and answers on a
    // url its path does not spell. The registry row's NAME is the target (which
    // is what `write_bundled` copies to and what the manifest carries), and its
    // SOURCE is still the path.
    let root = estate_root("target");
    let (values, _, bundled) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _url = const asset::bundle_as("static/robots.txt", "/robots.txt");
        }
        main();
        "#,
        &root,
    );
    assert_eq!(
        values,
        vec![vilan_core::interpreter::ConstValue::Str(
            "/robots.txt".to_string()
        )],
        "the call folds to the target url, which is what a `<link href>` wants"
    );
    assert_eq!(bundled.len(), 1);
    assert_eq!(
        bundled[0].1, "robots.txt",
        "the output name is the target, not the path: {bundled:?}"
    );
    assert!(
        bundled[0].0.ends_with("static/robots.txt"),
        "and the source is still where the file actually is: {bundled:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_bundle_as_url_shape_is_refused() {
    // The url the call returns IS the url the copy answers on, so every shape
    // where the url spelled and the file served would part company is refused
    // with the fix named. Each row is a distinct rule: one message for all of
    // them would make three of these vacuous.
    let root = estate_root("urlshape");
    for (url, expected) in [
        (
            "robots.txt",
            "urls start at the site root; `robots.txt` does not — write `/robots.txt`",
        ),
        ("/a\\\\b.txt", "urls are `/`-separated on every host"),
        ("/a//b.txt", "has an empty segment"),
        ("/a/./b.txt", "has a `.` segment"),
        ("/a/../b.txt", "has a `..` segment"),
        ("/", "has an empty segment"),
    ] {
        let errors = const_errors(
            Box::leak(
                format!(
                    r#"
        import std::asset;
        fun main() {{
            let _url = const asset::bundle_as("static/robots.txt", "{url}");
        }}
        main();
        "#
                )
                .into_boxed_str(),
            ),
            &root,
        );
        assert_named(&errors, expected);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn two_sources_claiming_one_target_name_both() {
    // The refusal the identity rule used to give for free: under `bundle` the
    // path WAS the name, so two files could never claim one. The diagnostic
    // carries BOTH sources, because a collision is a statement about a pair and
    // naming one half of it sends the reader to the wrong call.
    let root = estate_root("collision");
    let errors = const_errors(
        r#"
        import std::asset;
        fun main() {
            let _one = const asset::bundle_as("static/robots.txt", "/pinned.txt");
            let _two = const asset::bundle_as("static/logo.png", "/pinned.txt");
        }
        main();
        "#,
        &root,
    );
    assert_named(
        &errors,
        "`static/robots.txt` and `static/logo.png` both bundle to `/pinned.txt`",
    );
    assert_named(&errors, "give one of them a target of its own");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_same_source_and_target_twice_registers_once() {
    // `bundle`'s dedup, unchanged by the target: one file at one url is one
    // copy, one manifest row and one `serve_build` route, however many call
    // sites named it.
    let root = estate_root("dedup");
    let (_, _, bundled) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _one = const asset::bundle_as("static/robots.txt", "/robots.txt");
            let _two = const asset::bundle_as("./static/robots.txt", "/robots.txt");
        }
        main();
        "#,
        &root,
    );
    assert_eq!(
        bundled.len(),
        1,
        "one (source, target) pair is one registration, `./` and all: {bundled:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_digest_is_the_sha256_of_the_files_bytes() {
    // Against the canonical vector: sha-256("abc"). Lowercase hex, 64
    // characters, of the BYTES — a fingerprint of a font or a `.png` must be a
    // fingerprint of the file and not of a decoding.
    let root = estate_root("digest");
    let (values, _, _) = const_bundles(
        r#"
        import std::asset;
        fun main() {
            let _hex = const asset::digest("static/logo.png");
        }
        main();
        "#,
        &root,
    );
    assert_eq!(
        values,
        vec![vilan_core::interpreter::ConstValue::Str(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_string()
        )],
        "the file holds `abc`, whose sha-256 is the standard test vector"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_digested_file_is_a_tracked_build_input() {
    // Tracked exactly as `read`'s and `bundle`'s are, and it has to be: a
    // fingerprinted url that did not re-mint when the file changed would serve
    // last round's bytes under a name that promises they are immutable — the
    // single worst failure the cache tier this exists for can have.
    let root = estate_root("digesttrack");
    let source = r#"
        import std::asset;
        fun main() {
            let _hex = const asset::digest("static/logo.png");
        }
        main();
        "#;
    let (first, before, _) = const_bundles(source, &root);
    std::fs::write(root.join("static/logo.png"), "abcd").expect("edit the file");
    let (second, after, _) = const_bundles(source, &root);
    assert_ne!(first, second, "the digest must follow the bytes");
    let key = |inputs: &[(PathBuf, Option<u64>)]| {
        inputs
            .iter()
            .find(|(path, _)| *path == root.join("static/logo.png"))
            .expect("the digested file is recorded")
            .1
    };
    assert_ne!(
        key(&before),
        key(&after),
        "and its input record must move with them: {before:?} {after:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_runtime_digest_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::asset;
        fun main() {
            let _hex = asset::digest("logo.png");
        }
        main();
        "#,
        r#"asset::digest("logo.png")"#,
        "compile-time-only",
    );
    assert_fails_with(
        r#"
        import std::asset;
        fun fingerprint(): str {
            asset::digest("logo.png")
        }
        fun main() {
            let _hex = fingerprint();
        }
        main();
        "#,
        "`fingerprint` (it reaches `asset::digest`) is compile-time-only",
    );
}

#[test]
fn the_estate_recipe_folds_to_one_url_per_file() {
    // 035's recipe, whole: enumerate, rewrite the url as ordinary code, bundle
    // each file at the url that rewrite produced — with the last one
    // fingerprinted, which is 024's exhibit (kolt's fingerprints minted out of
    // band) closed. Three verbs in one program, because what each is for is the
    // other two.
    let root = estate_root("recipe");
    let (values, _, bundled) = const_bundles(
        r#"
        import std::asset;
        fun estate(): List<str> {
            mut urls: List<str> = [];
            for file in asset::read_dir_all("static") {
                urls.push(asset::bundle_as(i"static/{file}", i"/{file}"));
            }
            urls.push(asset::bundle_as(
                "static/logo.png",
                i"/hashed/logo.{asset::digest("static/logo.png").substring(0, 8)}.png",
            ));
            urls
        }
        fun main() {
            let _estate = const estate();
        }
        main();
        "#,
        &root,
    );
    assert_eq!(
        strings(&values[0]),
        vec![
            "/icons/close.svg",
            "/icons/open.svg",
            "/logo.png",
            "/robots.txt",
            // sha-256("abc") begins `ba7816bf`.
            "/hashed/logo.ba7816bf.png",
        ],
        "every file lands at the url the loop minted, the prefix stripped"
    );
    let names: Vec<&str> = bundled.iter().map(|(_, name)| name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "icons/close.svg",
            "icons/open.svg",
            "logo.png",
            "robots.txt",
            "hashed/logo.ba7816bf.png",
        ],
        "and the build carries one copy per url: {bundled:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn serve_builds_content_type_reads_the_extension_through_path_extname() {
    // `content_type_of` used to be `file.split(".").last()` — a hand-rolled
    // `extname` carrying `extname`'s classic bug: a DOTFILE's leading dot read
    // as a type. `dist/.css` is a hidden file with no extension, and typing it
    // `text/css` would serve a file the table has no row for. It now goes
    // through `path::extname` (kolt.local 017), which answers `""` there.
    //
    // The subdirectory cases below are newly reachable: a bundled resource
    // keeps its package-relative path (kolt.local 029), so `content_type_of`
    // now sees paths with directories in them for the first time.
    assert_compiles_and_runs(
        r#"
        import std::build::content_type_of;
        import std::io::print;
        import std::option::Option::{ None, Some };
        fun name(file: str): str {
            match content_type_of(file) {
                Some(let content_type) => content_type
                None => "none"
            }
        }
        fun main() {
            print(name("dist/static/logo.png"));
            print(name("dist/.css"));
            print(name("dist/vendor.d/README"));
            print(name("dist/FAVICON.ICO"));
            print(name("dist/a.b/c.woff2"));
        }
        main();
        "#,
        "image/png\nnone\nnone\nimage/x-icon\nfont/woff2\n",
    );
}
