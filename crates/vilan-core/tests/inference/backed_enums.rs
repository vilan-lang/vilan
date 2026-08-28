//! Backed enums (B76): the grammar, the trap arm, the conversions, `Wire`
//! serialization, and the coverage walk (B106, B111, B115, B118).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- B76: backed enums — the grammar and its validation ----------------------
//
// `proposal/backed-enums.md`, RATIFIED with §7.2 deferred. The discriminant
// production GENERALIZES: `= ( (-)? INTEGER | STRING )`. This is not a new kind
// of enum — a payload-free variant may carry a compile-time-constant scalar,
// and an enum whose variants carry one lowers to that scalar BARE. Everything
// B79 closed for the integer half applies unchanged to the string half; the
// rules below are the ones the string half adds.

#[test]
fn b76_a_string_backed_enum_lowers_to_its_bare_string() {
    // §3.5, the whole thesis: `Align::Start` IS `"flex-start"` at runtime,
    // exactly as `Ordering::Greater` IS `1` (P1). No array, no wrapper.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        fun main() { print(Align::Start); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(r#"console.log("flex-start")"#),
        "Align::Start should lower to the bare string, got:\n{javascript}"
    );
    assert!(
        !javascript.contains("[0]"),
        "no array form should survive, got:\n{javascript}"
    );
}

#[test]
fn b76_a_match_on_a_string_backing_is_the_same_chain_a_raw_str_gets() {
    // §1.4/P2: a string backing needs NO new codegen path. The emitted `match`
    // is character-for-character the chain a `match` over a raw `str` produces
    // — `scalar_variant_test` with a `js::Node::String` where the
    // `js::Node::Number` is — which is what makes the feature a widening rather
    // than a new lowering. Compared as WHOLE EMISSIONS, so a divergence
    // anywhere in the shape (a temp, a wrapper, a jump table) fails this.
    //
    // THE REFERENCE SHAPE MOVED, and §9.2's slice 2 asks that the weakening be
    // written on the pin rather than absorbed silently. Under §9's ratified (b)
    // the backed side's exhaustive match keeps its last variant test and gains a
    // trap `else`, so the raw side is given the same shape — every value tested,
    // a `_` arm last — instead of the two-test-plus-bare-`else` chain it used to
    // be compared against. The claim this protects survives in a marginally
    // WEAKER form: not "a backed match emits what a raw `str` match emits", but
    // "a backed match emits what a raw `str` match WITH A TRAP ARM emits". That
    // is still the whole of §1.4's point — the codegen path is shared, the trap
    // included; what it no longer says is that a backed match costs a raw one's
    // exact bytes.
    let backed = compile(
        r#"
        import std::print;
        enum Align { Start = "start", End = "end", Center = "center" }
        fun classify(a: Align): str {
            match a { Align::Start => "s", Align::End => "e", Align::Center => "c" }
        }
        fun main() { print(classify(Align::End)); }
        "#,
    )
    .expect("a clean compile");
    let raw = compile(
        r#"
        import std::print;
        fun classify(a: str): str {
            match a { "start" => "s", "end" => "e", "center" => "c", _ => "trap" }
        }
        fun main() { print(classify("end")); }
        "#,
    )
    .expect("a clean compile");
    // The reference side cannot SPELL the trap — `__enum_trap` is a compiler
    // helper, not a callable — so the two differences the trap arm makes are
    // reconciled here, and named rather than hidden: the helper's own
    // definition (emitted once per file, on demand) and the trap statement
    // standing where the raw match's `_` arm assigns. Everything the pin is
    // about — the chain, its temps, the tested constants, the arm the trap
    // occupies — is still compared byte for byte.
    let reference = format!(
        "function __enum_trap(name, value) {{\n\
         \tthrow name + \": \" + JSON.stringify(value) + \" is not one of its values\";\n\
         }}\n{}",
        raw.replace("$b = \"trap\";", "__enum_trap(\"Align\", $a);")
    );
    assert_eq!(
        backed, reference,
        "a backed enum's match should emit exactly what a raw `str` match with a trap arm emits"
    );
}

// --- B76 §9: the trap arm --------------------------------------------------
//
// `backed-enums.md` §9, candidate (b) RATIFIED 2026-08-09. A backed enum lowers
// to a bare host primitive, so §1.5's exhaustiveness proof is over the vilan
// VARIANT SET and never was a proof about the runtime value's domain — the
// host's. P12 measured where that gap is observable and it is one construct:
// `is`, `==` and a `_`-armed match all answer `false`/`_` for a value outside
// the set (honest), while an exhaustive match's last arm is a bare `else` that
// answers with a confident wrong variant (P11). So every exhaustive match over
// a backed enum now tests its last variant too and traps in the `else`.

#[test]
fn b76_an_exhaustive_string_backed_match_traps_instead_of_falling_through() {
    // §9.5 slice 1, the `str` backing. Every variant is tested — including the
    // last, which used to ride the bare `else` — and the `else` names the enum
    // and the raw value.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", Center = "center", End = "flex-end" }
        fun label(align: Align): str {
            match align { Align::Start => "s", Align::Center => "c", Align::End => "e" }
        }
        fun main() { print(label(Align::Center)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(r#"} else if ($a === "flex-end") {"#),
        "the LAST variant should get its own test, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\t} else {\n\t\t__enum_trap(\"Align\", $a);\n\t}"),
        "the `else` should be the trap, got:\n{javascript}"
    );
    assert!(
        javascript.contains("function __enum_trap(name, value) {"),
        "the trap helper should be emitted on demand, got:\n{javascript}"
    );
}

#[test]
fn b76_an_exhaustive_integer_backed_match_traps_too() {
    // §9.5 slice 1, the integer backing. §9's ruling is about the host
    // boundary, not about strings — P14 measured the same delta on `Ordering`,
    // the one backed enum std already shipped, and `vilan/test/enum-discriminant.mjs`
    // is the corpus golden that moved for it.
    let javascript = compile(
        r#"
        import std::print;
        enum Ordering { Less = -1, Equal = 0, Greater = 1 }
        fun describe(order: Ordering): str {
            match order { Ordering::Less => "less", Ordering::Equal => "equal", Ordering::Greater => "greater" }
        }
        fun main() { print(describe(Ordering::Greater)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("} else if ($a === 1) {"),
        "the LAST variant should get its own test, got:\n{javascript}"
    );
    assert!(
        javascript.contains("__enum_trap(\"Ordering\", $a);"),
        "the `else` should be the trap, got:\n{javascript}"
    );
}

#[test]
fn b76_the_trap_arm_is_the_exhaustive_case_only() {
    // §9's scope, from the other side. A match the author already gave a
    // catch-all is not exhaustive-by-variants: its `_` is a real arm the author
    // wrote and means, an out-of-set value takes it, and P12 calls that answer
    // honest. Nothing changes — no extra test, no trap, no helper.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", Center = "center", End = "flex-end" }
        fun label(align: Align): str {
            match align { Align::Start => "s", _ => "other" }
        }
        fun main() { print(label(Align::End)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        !javascript.contains("__enum_trap"),
        "a match with a user `_` arm should keep its bare `else`, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\t} else {\n\t\t$b = \"other\";\n\t}"),
        "the user's arm should still be the bare `else`, got:\n{javascript}"
    );
}

#[test]
fn b76_the_trap_arm_reaches_only_backed_enums() {
    // §3.1(b)'s conjunction decides this, as it decides the lowering: an enum
    // without a backing value keeps the `[index, ..data]` array form, whose
    // `[0]` slot the language itself writes, so its exhaustiveness proof IS a
    // proof about the runtime value and its bare `else` stays honest. `bool`
    // lowers to a native scalar through its own special case rather than a
    // backing value (§3.4 rejects a `bool` backing) and is not covered either.
    let javascript = compile(
        r#"
        import std::print;
        import std::option::Option;
        enum Plain { A, B, C }
        fun plain(p: Plain): str { match p { Plain::A => "a", Plain::B => "b", Plain::C => "c" } }
        fun boolean(b: bool): str { match b { true => "t", false => "f" } }
        fun opt(o: Option<i32>): i32 { match o { Option::Some(let n) => n, Option::None => 0 } }
        fun main() { print(plain(Plain::C)); print(boolean(false)); print(opt(Option::Some(3))); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        !javascript.contains("__enum_trap"),
        "only a BACKED enum's match should trap, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\t} else {\n\t\t$b = \"c\";\n\t}"),
        "the plain enum's last arm should still be the bare `else`, got:\n{javascript}"
    );
}

#[test]
fn b76_the_trap_arm_fires_on_a_host_supplied_value() {
    // The behavioral half, and B107's own repro (§9.2's P16): a function-typed
    // parameter's parameter is a return position wearing a parameter's clothes
    // — the HOST constructs the value — and before this the program printed
    // `e`, `Align::End`, confidently, exit 0. It now trap. `forEach` is the
    // host helper: it calls the handler with each element, so the second call
    // hands vilan a value outside `Align`'s set with nothing in between.
    let (stdout, stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        enum Align { Start = "flex-start", Center = "center", End = "flex-end" }
        [extern("Array.prototype.forEach.call")]
        external fun for_each_align(values: List<str>, handler: |Align| void): void;
        fun label(align: Align): str {
            match align { Align::Start => "s", Align::Center => "c", Align::End => "e" }
        }
        fun main() {
            for_each_align([ "center" ], |a| print(label(a)));
            for_each_align([ "middle" ], |a| print(label(a)));
        }
        "#,
    );
    assert_eq!(
        stdout, "c\n",
        "the in-set value should still label normally"
    );
    assert!(
        stderr.contains(r#"Align: "middle" is not one of its values"#),
        "the trap should name the enum and the raw value, got:\n{stderr}"
    );
    assert_ne!(code, 0, "a trapped program must not exit 0");
}

#[test]
fn b76_the_trap_arm_reaches_the_edge_shapes_of_a_match() {
    // The forms a one-happy-path pin would miss, all three in one emission.
    //
    // A ONE-VARIANT enum: the single arm used to BE the bare `else`, so the
    // match emitted no test at all — now it tests and traps.
    //
    // A GUARDED final leg: the guard is still dropped, as it always was, and
    // only the pattern test is kept. The trap answers for values outside the
    // set, not for a guard that rejects an in-set one, so `Center if flag`
    // followed by a bare `Center` behaves exactly as before.
    //
    // The SEQUENCE emission (B59): a guard needing statement slots turns the
    // whole match into a flat `matched`-flag sequence rather than an else-if
    // chain, a second emitter that has to grow the arm too. There the trap is
    // the final `if (!matched)`.
    let javascript = compile(
        r#"
        import std::print;
        import std::option::Option;
        enum One { Only = "only" }
        enum Align { Start = "flex-start", Center = "center", End = "flex-end" }
        fun single(o: One): str { match o { One::Only => "o" } }
        fun guarded(a: Align, flag: bool): str {
            match a {
                Align::Start => "s",
                Align::Center if flag => "c!",
                Align::Center => "c",
                Align::End => "e",
            }
        }
        fun sequenced(a: Align, o: Option<i32>): str {
            match a {
                Align::Start => "s",
                Align::Center if o is Option::Some(let n) && n > 1 => "c!",
                Align::Center => "c",
                Align::End => "e",
            }
        }
        fun main() {
            print(single(One::Only));
            print(guarded(Align::End, false));
            print(sequenced(Align::End, Option::None));
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("\tif ($a === \"only\") {\n\t\t$b = \"o\";\n\t} else {\n\t\t__enum_trap(\"One\", $a);\n\t}"),
        "a one-variant match should test its single arm and trap, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\t} else if ($c === \"flex-end\") {\n\t\t$d = \"e\";\n\t} else {\n\t\t__enum_trap(\"Align\", $c);\n\t}"),
        "a guarded match's final leg should keep only its PATTERN test, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\tif (!($h)) {\n\t\t__enum_trap(\"Align\", $e);\n\t}"),
        "the sequence emission should trap on the unmatched flag, got:\n{javascript}"
    );
    // And it still runs the in-set values the way it always did.
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "o\ne\ne\n",
        "the in-set paths must be unchanged"
    );
}

#[test]
fn b114_the_trap_arm_reaches_a_nested_backed_pattern() {
    // §11.6's residual, closed. `match p { Pair::Of(Align::Start) => .. }`
    // dropped the LAST leg's whole condition, nested variant test included, so
    // an out-of-set `Align` inside the payload landed on the last arm
    // confidently. The trap question is now asked of the pattern TREE, and the
    // value it names is the one the dropped test compared — the payload slot,
    // not the subject.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e" }
        }
        fun main() { print(label(Pair::Of(Align::End))); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "\t} else if ($a[0] === 0 && $a[1] === \"flex-end\") {\n\t\t$b = \"e\";\n\
             \t} else {\n\t\t__enum_trap(\"Align\", $a[1]);\n\t}"
        ),
        "the final leg should keep its nested test and trap at the PAYLOAD slot, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "e\n",
        "the in-set path must be unchanged"
    );
}

#[test]
fn b114_a_nested_trap_names_the_out_of_set_payload() {
    // The behavior the emission buys: driven with a payload the host invented,
    // the match says so instead of answering `Align::End`. `__enum_trap` throws
    // a bare string, which is `panic()`'s shape.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e" }
        }
        fun main() { print(label(Pair::Of(Align::Start))); }
        "#,
    )
    .expect("a clean compile");
    let driven = format!(
        "{javascript}\ntry {{ label([ 0, \"middle\" ]); }} catch (error) {{ console.log(error); }}\n"
    );
    assert_eq!(
        run_js(&driven).expect("a clean run"),
        "s\nAlign: \"middle\" is not one of its values\n",
        "the nested trap should name the enum and the raw payload value"
    );
}

#[test]
fn b114_a_trap_reads_through_every_payload_it_is_nested_under() {
    // Two levels of payload, and a TUPLE subject — the two other ways a backed
    // test rides in a condition. The accessor the trap names is
    // `compile_pattern`'s own, so it tracks the nesting exactly.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Mid { Of(Align) }
        enum Outer { Of(Mid) }
        fun deep(o: Outer): str {
            match o { Outer::Of(Mid::Of(Align::Start)) => "s", Outer::Of(Mid::Of(Align::End)) => "e" }
        }
        fun paired(a: Align, flag: bool): str {
            match (a, flag) {
                (Align::Start, true) => "st",
                (Align::Start, false) => "sf",
                (Align::End, true) => "et",
                (Align::End, false) => "ef",
            }
        }
        fun main() {
            print(deep(Outer::Of(Mid::Of(Align::End))));
            print(paired(Align::End, false));
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("__enum_trap(\"Align\", $a[1][1]);"),
        "a twice-nested payload should trap at the inner slot, got:\n{javascript}"
    );
    assert!(
        javascript.contains("__enum_trap(\"Align\", $c[0]);"),
        "a tuple subject should trap at the tuple's own element, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "e\nef\n",
        "the in-set paths must be unchanged"
    );
}

#[test]
fn b114_several_backed_tests_in_one_leg_name_the_one_that_left_its_set() {
    // The question §11.6 filed as needing a message design §9 does not have:
    // `Two::Of(Align::End, Display::Inline)` carries TWO backed tests, and which
    // of them failed is not knowable from the leg's condition. It IS knowable by
    // asking each value whether it is in its enum's set AT ALL, which is a
    // different question from "did this leg match" — so §9's message stands and
    // the trap block orders the tests instead. The last needs no membership
    // check: it is what is left when none of the others answered.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Display { Block = "block", Inline = "inline" }
        enum Two { Of(Align, Display) }
        fun label(t: Two): str {
            match t {
                Two::Of(Align::Start, Display::Block) => "sb",
                Two::Of(Align::Start, Display::Inline) => "si",
                Two::Of(Align::End, Display::Block) => "eb",
                Two::Of(Align::End, Display::Inline) => "ei",
            }
        }
        fun main() { print(label(Two::Of(Align::End, Display::Inline))); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "\t} else {\n\
             \t\tif (!($a[1] === \"flex-start\" || $a[1] === \"flex-end\")) {\n\
             \t\t\t__enum_trap(\"Align\", $a[1]);\n\
             \t\t}\n\
             \t\t__enum_trap(\"Display\", $a[2]);\n\t}"
        ),
        "the trap should ask each backed test for SET MEMBERSHIP in order, got:\n{javascript}"
    );
    let driven = format!(
        "{javascript}\n\
         try {{ label([ 0, \"middle\", \"inline\" ]); }} catch (error) {{ console.log(error); }}\n\
         try {{ label([ 0, \"flex-end\", \"grid\" ]); }} catch (error) {{ console.log(error); }}\n"
    );
    assert_eq!(
        run_js(&driven).expect("a clean run"),
        "ei\n\
         Align: \"middle\" is not one of its values\n\
         Display: \"grid\" is not one of its values\n",
        "each direction should name the value that actually left its set"
    );
}

#[test]
fn b114_a_nested_backed_test_reaches_the_sequence_emitter_too() {
    // B59's flat `matched`-flag emission is the second shape a match takes, and
    // §11.3 had to teach the top-level trap about it. The nested trap arrives
    // through the same appended leg, so it needs to learn nothing.
    let javascript = compile(
        r#"
        import std::print;
        import std::option::Option;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align) }
        fun label(p: Pair, o: Option<i32>): str {
            match p {
                Pair::Of(Align::Start) if o is Option::Some(let n) && n > 1 => "s!",
                Pair::Of(Align::Start) => "s",
                Pair::Of(Align::End) => "e",
            }
        }
        fun main() { print(label(Pair::Of(Align::End), Option::None)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("\tif (!($d)) {\n\t\t__enum_trap(\"Align\", $a[1]);\n\t}"),
        "the sequence emission should trap at the nested slot, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "e\n",
        "the in-set path must be unchanged"
    );
}

#[test]
fn b114_a_match_carrying_no_backed_test_keeps_its_bare_else() {
    // The NARROW rule, which is the true one. §11.6 warned the generalization
    // "changes the emission of matches over UNBACKED enums", and it does not:
    // the trap keys on a BACKED test, and an unbacked enum's discriminant is the
    // compiler's own — its runtime domain IS its variant set, so there is
    // nothing outside it to name. A nested LITERAL is the same argument for a
    // primitive. Both keep the bare `else` they have always had, which is why
    // the whole corpus moved zero bytes.
    //
    // B118 edited the literal half's program, not its claim. `Wrapped::Of(1),
    // Wrapped::Of(2)` was accepted as total over an `i32` payload — shape (c)
    // of the coverage walk's holes — so the leg carrying the literal is now a
    // leg BEFORE an irrefutable one. It keeps its test either way, and the
    // final leg still drops its condition with no trap behind it, which is all
    // the narrow rule ever claimed here.
    let javascript = compile(
        r#"
        import std::print;
        enum Inner { A, B }
        enum Pair { Of(Inner) }
        enum Wrapped { Of(i32) }
        fun nested(p: Pair): str {
            match p { Pair::Of(Inner::A) => "a", Pair::Of(Inner::B) => "b" }
        }
        fun literal(w: Wrapped): str {
            match w { Wrapped::Of(1) => "one", Wrapped::Of(let n) => "many" }
        }
        fun main() { print(nested(Pair::Of(Inner::B))); print(literal(Wrapped::Of(2))); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        !javascript.contains("__enum_trap"),
        "an unbacked nested test must not grow a trap, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\tif ($a[0] === 0 && $a[1][0] === 0) {\n\t\t$b = \"a\";\n\t} else {"),
        "the unbacked final leg should still drop its condition, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\tif ($c[0] === 0 && $c[1] === 1) {\n\t\t$d = \"one\";\n\t} else {"),
        "the literal leg should keep its nested test, got:\n{javascript}"
    );
}

#[test]
fn b114_a_written_catch_all_is_still_the_authors_own_arm() {
    // P13's rule, unchanged one level down: a `_` the author wrote IS the trap
    // arm, and the compiler must not add a second one behind it.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", _ => "other" }
        }
        fun main() { print(label(Pair::Of(Align::Start))); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        !javascript.contains("__enum_trap"),
        "a written catch-all needs no trap behind it, got:\n{javascript}"
    );
}

#[test]
fn b121_a_backed_test_in_an_earlier_leg_reaches_the_bare_else() {
    // §12.4's filed hazard, closed (backed-enums.md §13). The final leg
    // carries no backed test, so `trap_tests` (the §12.1 mechanism) is empty
    // and the leg's own condition is still dropped — but `Of`'s two legs,
    // together, are its ONLY handler, both testing a specific `Align`
    // literal. Reaching this point with the subject's tag actually `Of`
    // means neither literal matched, which is possible only when the payload
    // left `Align`'s set. The fix re-dispatches on the tag INSIDE the dropped
    // leg's body: `Of` traps, and only the tag that owns the leg (`Other`)
    // still runs the author's own arm.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align), Other }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e", Pair::Other => "o" }
        }
        fun main() { print(label(Pair::Other)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "\t} else {\n\
             \t\tif ($a[0] === 0) {\n\
             \t\t\t__enum_trap(\"Align\", $a[1]);\n\
             \t\t} else {\n\
             \t\t\t$b = \"o\";\n\
             \t\t}\n\
             \t}"
        ),
        "the bare `else` should re-dispatch on the tag, trapping `Of` and \
         keeping `Other`'s own arm underneath, got:\n{javascript}"
    );
    assert_eq!(
        run_js(&javascript).expect("a clean run"),
        "o\n",
        "the legitimate `Pair::Other` path must be unchanged"
    );
}

#[test]
fn b121_an_out_of_set_payload_in_an_earlier_leg_traps_instead_of_misfiling() {
    // The behavior the emission buys: a host-invented `Of` payload traps
    // instead of silently answering `Other`. Driven both ways, alongside the
    // in-set control the analyzer itself cannot construct as `Pair::Of` with
    // a bad `Align` — only `[0, "middle"]`, built by hand, can.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align), Other }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e", Pair::Other => "o" }
        }
        fun main() {
            print(label(Pair::Of(Align::Start)));
            print(label(Pair::Of(Align::End)));
            print(label(Pair::Other));
        }
        "#,
    )
    .expect("a clean compile");
    let driven = format!(
        "{javascript}\ntry {{ label([ 0, \"middle\" ]); }} catch (error) {{ console.log(error); }}\n"
    );
    assert_eq!(
        run_js(&driven).expect("a clean run"),
        "s\ne\no\nAlign: \"middle\" is not one of its values\n",
        "every in-set path stays unchanged and the out-of-set `Of` payload \
         names the enum and the raw value, not `Pair::Other`"
    );
}

#[test]
fn b121_two_partitioned_variants_each_trap_their_own_enum() {
    // Generalizes the anchor to TWO tags each exhausted by backed literals
    // (`Of`/`Align`, `Alt`/`Display`), with `Other` the true bare leg. The
    // re-dispatch chains in the order the tags first appear, and each traps
    // only its own enum — the K=1 shape per tag, §12.1's format unchanged.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Display { Block = "block", Inline = "inline" }
        enum Pair { Of(Align), Alt(Display), Other }
        fun label(p: Pair): str {
            match p {
                Pair::Of(Align::Start) => "s",
                Pair::Of(Align::End) => "e",
                Pair::Alt(Display::Block) => "b",
                Pair::Alt(Display::Inline) => "i",
                Pair::Other => "o",
            }
        }
        fun main() { print(label(Pair::Other)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(
            "\t} else {\n\
             \t\tif ($a[0] === 0) {\n\
             \t\t\t__enum_trap(\"Align\", $a[1]);\n\
             \t\t} else if ($a[0] === 1) {\n\
             \t\t\t__enum_trap(\"Display\", $a[1]);\n\
             \t\t} else {\n\
             \t\t\t$b = \"o\";\n\
             \t\t}\n\
             \t}"
        ),
        "both partitioned tags should chain, in declaration order, each \
         trapping its own enum, got:\n{javascript}"
    );
    let driven = format!(
        "{javascript}\n\
         try {{ label([ 0, \"middle\" ]); }} catch (error) {{ console.log(error); }}\n\
         try {{ label([ 1, \"grid\" ]); }} catch (error) {{ console.log(error); }}\n\
         console.log(label([ 2 ]));\n"
    );
    assert_eq!(
        run_js(&driven).expect("a clean run"),
        "o\n\
         Align: \"middle\" is not one of its values\n\
         Display: \"grid\" is not one of its values\n\
         o\n",
        "each tag should trap its own enum, and the true bare leg is unaffected"
    );
}

#[test]
fn b121_a_variant_with_a_written_catch_all_payload_never_reaches_the_trap() {
    // The mechanism's own boundary: a tag covered by an IRREFUTABLE payload
    // leg of its own (`Pair::Of(let _)`) already matches THAT leg earlier in
    // the else-if chain, so the re-dispatch's `Of` branch is unreachable —
    // present in the source (harmless dead code) but never the path taken.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align), Other }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(let _) => "x", Pair::Other => "o" }
        }
        fun main() {
            print(label(Pair::Of(Align::End)));
            print(label(Pair::Other));
        }
        "#,
    )
    .expect("a clean compile");
    let driven = format!("{javascript}\nconsole.log(label([ 0, \"middle\" ]));\n");
    assert_eq!(
        run_js(&driven).expect("a clean run"),
        "x\no\nx\n",
        "an out-of-set `Align` under a leg that already covers `Of` unconditionally \
         takes that leg, same as any other value — it never reaches the trap"
    );
}

#[test]
fn b121_earlier_legs_over_an_unbacked_nested_enum_keep_the_bare_else() {
    // §12.2's narrow rule extended to the new mechanism: an UNBACKED nested
    // enum's runtime domain IS its variant set (the language wrote the
    // discriminant), so `Of`'s two legs testing `Inner::A`/`Inner::B` carry
    // no `BackedTest` at all — `earlier_variant_traps` never gets an entry
    // for `Of`, and the final leg's bare `else` is exactly what it always
    // was, byte for byte.
    let javascript = compile(
        r#"
        import std::print;
        enum Inner { A, B }
        enum Pair { Of(Inner), Other }
        fun label(p: Pair): str {
            match p { Pair::Of(Inner::A) => "a", Pair::Of(Inner::B) => "b", Pair::Other => "o" }
        }
        fun main() { print(label(Pair::Other)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        !javascript.contains("__enum_trap"),
        "an all-unbacked match needs no trap of any kind, got:\n{javascript}"
    );
    assert!(
        javascript.contains("\t} else {\n\t\t$b = \"o\";\n\t}"),
        "the final leg's bare `else` should be untouched, got:\n{javascript}"
    );
}

#[test]
fn b114_a_refutable_nested_pattern_is_not_exhaustive() {
    // Found probing B114, and it is the analyzer's, not the trap's: `Pair` has
    // one variant, so §1.5's by-name check called this total and the single leg
    // became the bare `else`. `Pair::Of(Inner::B)` then ran the `Inner::A` arm.
    // The right answer is a compile error naming the missing case — a trap
    // would paper over a missing check with a runtime throw. Closed by B118's
    // coverage walk, which asks the pattern TREE rather than its root.
    assert_fails_with(
        r#"
        import std::print;
        enum Inner { A, B }
        enum Pair { Of(Inner) }
        fun label(p: Pair): str {
            match p { Pair::Of(Inner::A) => "a" }
        }
        fun main() { print(label(Pair::Of(Inner::B))); }
        "#,
        "match is not exhaustive: missing Pair::Of(Inner::B)",
    );
}

#[test]
fn b76_a_string_backed_enum_runs() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end", Center = "center" }
        fun main() {
            let a = Align::End;
            print(match a { Align::Start => "s", Align::End => "e", Align::Center => "c" });
            print(a == Align::End);
            print(a != Align::Start);
            print(a is Align::End);
        }
        "#,
        "e\ntrue\ntrue\ntrue\n",
    );
}

#[test]
fn b76_a_backing_value_is_not_a_second_spelling_of_the_variant() {
    // §1.5/§3.7: exhaustiveness is checked on the variant SET by name, and a
    // raw literal pattern stays an error for strings exactly as it already is
    // for integers. The backing value is a representation, not a name.
    assert_fails_with(
        r#"
        enum Align { Start = "start", End = "end" }
        fun f(a: Align): i32 { match a { "start" => 0, _ => 1 } }
        fun main() { }
        "#,
        "cannot match type",
    );
    assert_fails_with(
        r#"
        enum Align { Start = "start", End = "end" }
        fun f(a: Align): i32 { match a { Align::Start => 0 } }
        fun main() { }
        "#,
        "match is not exhaustive: missing 'End'",
    );
}

#[test]
fn b76_mixed_backings_in_one_enum_are_rejected() {
    // §3.2. An enum has ONE runtime representation; a value that is sometimes a
    // number and sometimes a string is not a vilan type, and `.value()` would
    // have no return type. Both directions, because either literal could be the
    // typo — the message names both variants and both spellings.
    assert_fails_noting(
        r#"
        enum X { A = 1, B = "two" }
        fun main() { }
        "#,
        "variant 'B' is backed by a string (`\"two\"`) where 'A' is backed by an integer",
        "1",
        "'A' backs 'X' with an integer",
    );
    assert_fails_noting(
        r#"
        enum Y { A = "one", B = 2 }
        fun main() { }
        "#,
        "variant 'B' is backed by an integer (`2`) where 'A' is backed by a string",
        "\"one\"",
        "'A' backs 'Y' with a string",
    );
}

#[test]
fn b76_a_string_backing_must_be_written_on_every_variant() {
    // §3.1(a). C-style auto-increment is meaningful for integers; there is no
    // successor of `"start"`. Deriving the string from the variant NAME is the
    // rejected alternative (§2.1's evidence: five of std's eleven CSS enums have
    // names no case convention produces).
    assert_fails_noting(
        r#"
        enum X { A = "a", B }
        fun main() { }
        "#,
        "variant 'B' has no backing value, and a string backing has no successor",
        "\"a\"",
        "'A' backs 'X' with a string here",
    );
    // The integer half is untouched — the sequence continues as it always has.
    assert_compiles(
        r#"
        enum Y { A = 5, B, C }
        fun main() { }
        "#,
    );
}

#[test]
fn b76_two_variants_cannot_share_a_string_backing() {
    // §3.7, B79's uniqueness rule widened. Two variants sharing a value ARE one
    // runtime value: the second `match` arm is unreachable and an exhaustive
    // match returns the wrong answer with exit 0. A CSS keyword collision is a
    // typo, not an exotic input — std writes `Display::Hidden => "none"` and
    // `UserSelect::Off => "none"` in different enums today.
    assert_fails_noting(
        r#"
        enum Align { Start = "a", End = "a" }
        fun main() { }
        "#,
        "variant 'End' has backing value \"a\", which 'Start' already uses",
        "Start = \"a\"",
        "'Start' has backing value \"a\"",
    );
    // The same value in two DIFFERENT enums stays legal — that is the shape
    // std's `Display::Hidden` / `UserSelect::Off` pair really has.
    assert_compiles(
        r#"
        enum Display { Hidden = "none", Flex = "flex" }
        enum UserSelect { Off = "none", All = "all" }
        fun main() { }
        "#,
    );
}

#[test]
fn b76_a_payload_variant_cannot_carry_a_string_backing() {
    // §3.3, the string half of the rule B79 shipped for integers. A bare
    // backing value has nowhere to put a payload.
    assert_fails_with(
        r#"
        enum X { A(str) = "a", B = "b" }
        fun main() { }
        "#,
        "variant 'A' carries a payload, so it cannot have an explicit backing value",
    );
    assert_fails_noting(
        r#"
        enum Y { A = "a", B(i32) }
        fun main() { }
        "#,
        "an explicit backing value is only meaningful when every variant is data-less, and 'B' \
         carries a payload",
        "B(i32)",
        "'B' carries a payload here",
    );
}

#[test]
fn b76_the_backing_type_set_is_str_and_the_integers() {
    // §3.4. Floats are rejected by the integer rule (their `===` lowering and
    // `NaN !== NaN` would break both the duplicate check and every variant
    // test); `bool` never reaches the production at all, `bool` being itself an
    // enum that already lowers to native `true`/`false`. The production states
    // its own set rather than deferring to a later rule.
    assert_fails_with(
        r#"
        enum X { A = true, B = false }
        fun main() { }
        "#,
        "expected an integer, a string",
    );
    assert_fails_with(
        r#"
        enum Y { A = 1.5 }
        fun main() { }
        "#,
        "an enum backing value must be an integer or a string, and `1.5` is neither",
    );
}

#[test]
fn b76_ordering_operators_are_rejected_on_a_string_backing() {
    // §3.6. `Size::Large < Size::Small` would be TRUE because `"lg" < "sm"`,
    // and the thing a reader means — order by declaration index — cannot be
    // provided, because bare lowering erases the index at runtime. `==`/`!=`
    // stay; the integer form is untouched (P8).
    for operator in ["<", "<=", ">", ">="] {
        assert_fails_with(
            &format!(
                r#"
                enum Size {{ Large = "lg", Small = "sm" }}
                fun f(a: Size, b: Size): bool {{ a {operator} b }}
                fun main() {{ }}
                "#
            ),
            "`Size` is backed by strings, and a backing value is not an order",
        );
    }
    assert_compiles(
        r#"
        enum Size { Large = "lg", Small = "sm" }
        fun f(a: Size, b: Size): bool { a == b || a != b }
        fun main() { }
        "#,
    );
    // The integer backing still orders, which is what `std::compare`'s
    // `PartialOrd` defaults depend on.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Level { Low = 0, High = 1 }
        fun main() { print(Level::Low < Level::High); }
        "#,
        "true\n",
    );
}

#[test]
fn b76_adding_a_string_backing_does_not_move_the_integer_path() {
    // §5's premise, pinned: the grammar is WIDENED, not altered. Every existing
    // integer-discriminant enum compiles identically and emits identical
    // JavaScript.
    let javascript = compile(
        r#"
        import std::print;
        enum Ordering2 { Less = -1, Equal = 0, Greater = 1 }
        fun main() {
            print(match Ordering2::Greater {
                Ordering2::Less => "less",
                Ordering2::Equal => "equal",
                Ordering2::Greater => "greater",
            });
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("=== -1") && javascript.contains("=== 0"),
        "the integer chain should be unchanged, got:\n{javascript}"
    );
}

#[test]
fn b76_a_plain_enum_keeps_its_array_form() {
    // §3.1(b): the conjunction stays a conjunction. `enum Plain { A, B }` is
    // NOT backed, and adding `= 0` to one variant changes the representation of
    // the whole type — a wart, preserved deliberately, because changing it
    // would change the runtime representation of every payload-free enum in
    // every existing program.
    let javascript = compile(
        r#"
        import std::print;
        enum Plain { A, B }
        fun main() { print(match Plain::B { Plain::A => "a", Plain::B => "b" }); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("[0] === 0"),
        "a plain enum should keep the `[index]` array form, got:\n{javascript}"
    );
}

#[test]
fn b76_a_backing_string_keeps_its_escapes() {
    // The literal is carried raw and unescaped at emission like any other
    // string, so a backing value may hold whatever the host speaks.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Quoted { Tab = "a\tb", Quote = "say \"hi\"" }
        fun main() {
            print(match Quoted::Quote { Quoted::Tab => "tab", Quoted::Quote => "quote" });
            print(Quoted::Tab);
        }
        "#,
        "quote\na\tb\n",
    );
}

// --- B76: the conversions — `.value()` out, `Enum::parse` back ---------------
//
// §3.8, synthesized on every backed enum rather than opted into by a derive
// (§7.3 — the backing value is already the opt-in). Written as vilan source, so
// a user's own `value` meets B57's duplicate-inherent rule; `value()` still
// costs nothing, because the transformer folds the call to its receiver.

#[test]
fn b76_value_returns_the_backing_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Level { Low = -1, High = 2 }
        fun main() {
            print(Align::End.value());
            print(Level::Low.value());
        }
        "#,
        "flex-end\n-1\n",
    );
}

#[test]
fn b76_value_lowers_to_the_identity() {
    // The receiver already IS the backing value, so `value.value()` compiles to
    // `value` — that is what makes std's eleven CSS wrappers delete outright
    // instead of moving their `match` chains into the emitted output. The
    // folded-away body then has no callers and emits nothing.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        fun render(value: Align): str { value.value() }
        fun main() { print(render(Align::Start)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("return value;"),
        "`value.value()` should compile to `value`, got:\n{javascript}"
    );
    assert!(
        !javascript.contains("function value"),
        "the synthesized `value` body should emit nothing once folded, got:\n{javascript}"
    );
}

#[test]
fn b76_parse_round_trips_and_answers_none_outside_the_set() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Level { Low = -1, High = 2 }
        fun show(parsed: Option<Align>): str {
            match parsed { Option::Some(let a) => a.value(), Option::None => "none" }
        }
        fun main() {
            print(show(Align::parse("flex-start")));
            print(show(Align::parse("flex-end")));
            print(show(Align::parse("middle")));
            print(match Level::parse(2) {
                Option::Some(let l) => l.value(),
                Option::None => 0,
            });
            print(match Level::parse(7) { Option::Some(let l) => l.value(), Option::None => 0 });
        }
        "#,
        "flex-start\nflex-end\nnone\n2\n0\n",
    );
}

#[test]
fn b76_parse_needs_no_option_import_at_the_declaration() {
    // The synthesized block carries its own `Option`, so a module that declares
    // a backed enum and never mentions `Option` still compiles.
    assert_compiles(
        r#"
        enum Align { Start = "start", End = "end" }
        fun main() { }
        "#,
    );
}

#[test]
fn b76_a_user_declared_value_or_parse_is_a_hard_error() {
    // §3.8's collision rule, reached through B57 rather than through a rule of
    // its own: a synthesized member that quietly loses is worse than a visible
    // name clash. The note says the other declaration is the compiler's,
    // because there is no file to point at.
    assert_fails_with(
        r#"
        enum A { X = "x", Y = "y" }
        impl A { fun value(self): str { "nope" } }
        fun main() { }
        "#,
        "'value' is already defined for 'A'",
    );
    assert_fails_with(
        r#"
        enum B { X = "x", Y = "y" }
        impl B { fun parse(text: str): str { "nope" } }
        fun main() { }
        "#,
        "'parse' is already defined for 'B'",
    );
    // …and the note names the synthesized member rather than pointing the
    // author's own declaration back at itself.
    let diagnostics = failure_diagnostics_with_notes(
        r#"
        enum C { X = "x", Y = "y" }
        impl C { fun value(self): str { "nope" } }
        fun main() { }
        "#,
    );
    assert!(
        diagnostics.iter().any(|(_, _, note)| note
            .as_ref()
            .is_some_and(|(msg, _, _)| msg.contains("synthesized for 'C' by the compiler"))),
        "got: {diagnostics:#?}"
    );
}

#[test]
fn b76_an_unbacked_enum_gets_no_conversions() {
    // The negative space: `value`/`parse` exist because a backing value was
    // written, so a plain enum and a payload enum keep their surfaces free.
    assert_fails_with(
        r#"
        enum Plain { A, B }
        fun main() { let x = Plain::A.value(); }
        "#,
        "value",
    );
    assert_compiles(
        r#"
        enum Plain { A, B }
        impl Plain { fun value(self): i32 { 0 } }
        fun main() { let x = Plain::A.value(); }
        "#,
    );
}

#[test]
fn b76_a_wide_integer_backing_widens_the_conversion_type() {
    // The backing type is the narrowest plain-JS-number integer that holds
    // every discriminant: `i32` by default (the language's default integer, so
    // `Ordering::Greater.value() == 1` needs no suffix), `i53` when a
    // discriminant does not fit.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Wide { Big = 3000000000, Small = 1 }
        fun main() { print(Wide::Big.value()); }
        "#,
        "3000000000\n",
    );
}

#[test]
fn b76_std_ordering_gains_the_conversions() {
    // std's one pre-existing backed enum picks them up like any other, and its
    // `value()` is the identity over the bare discriminant.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::compare::Ordering;
        import std::option::Option;
        fun main() {
            print(Ordering::Greater.value());
            print(match Ordering::parse(-1) {
                Option::Some(let o) => o.value(),
                Option::None => 99,
            });
        }
        "#,
        "1\n-1\n",
    );
}

// --- B76: `Wire`/JSON serialize as the backing value -------------------------
//
// §3.9, and the one genuine format change in the arc. Today's derive keys on
// the variant NAME and ignores the discriminant entirely (P9: `Ordering::
// Greater` went on the wire as `"Greater"`), so this is a DIVERGENCE rather
// than an extension — taken now because §1.6 checked it costs nothing: there is
// no `[derive(Wire)]` or `[derive(Json)]` enum anywhere in `vilan/std/src/`.

#[test]
fn b76_a_backed_enum_encodes_as_its_backing_value() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::Json;
        [derive(Json)]
        enum Align { Start = "flex-start", End = "flex-end" }
        [derive(Json)]
        enum Code { Ok = 200, NotFound = 404 }
        fun main() {
            print(Align::Start.to_json());
            print(Code::NotFound.to_json());
        }
        "#,
        "\"flex-start\"\n404\n",
    );
}

#[test]
fn b76_a_backed_enum_decodes_from_its_backing_value() {
    // The reverse direction goes through the synthesized `parse`, so a value
    // outside the set is `Err` rather than a confidently wrong variant.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::{ Json, FromJson };
        import std::result::Result;
        [derive(Json)]
        enum Align { Start = "flex-start", End = "flex-end" }
        fun show(decoded: Result<Align, str>): str {
            match decoded { Result::Ok(let a) => a.value(), Result::Err(let e) => e }
        }
        fun main() {
            print(show(Align::from_json("\"flex-end\"")));
            print(show(Align::from_json("\"middle\"")));
        }
        "#,
        "flex-end\nunknown value in JSON for enum Align\n",
    );
}

#[test]
fn b76_a_backed_enum_round_trips_through_wire() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::json_codec;
        import std::result::Result;
        import std::wire::{ Wire, encode, decode };
        [derive(Wire)]
        enum Align { Start = "flex-start", End = "flex-end" }
        [derive(Wire)]
        struct Holder { align: Align, count: i32 }
        fun main() {
            let back: Result<Holder, str> =
                decode(json_codec(), encode(json_codec(), Holder { align = Align::End, count = 3 }));
            match back {
                Result::Ok(let holder) => print(holder.align.value()),
                Result::Err(let reason) => print(reason),
            }
            let bad: Result<Align, str> = decode(json_codec(), encode(json_codec(), "middle"));
            match bad {
                Result::Ok(let align) => print(align.value()),
                Result::Err(let reason) => print(reason),
            }
        }
        "#,
        "flex-end\nunknown value for enum Align\n",
    );
}

#[test]
fn b76_an_unbacked_enum_still_serializes_by_variant_name() {
    // The negative space, and the reason §3.9 is a divergence and not a
    // rewrite: an enum with no backing value keeps the externally-tagged form
    // exactly as before.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::{ Json, FromJson };
        import std::result::Result;
        [derive(Json)]
        enum Plain { A, B }
        [derive(Json)]
        enum Payload { Text(str), Count(i32) }
        fun main() {
            print(Plain::B.to_json());
            print(Payload::Text("hi").to_json());
            print(match Plain::from_json("\"A\"") {
                Result::Ok(Plain::A) => "a",
                Result::Ok(Plain::B) => "b",
                Result::Err(let e) => e,
            });
        }
        "#,
        "\"B\"\n{\"Text\":\"hi\"}\na\n",
    );
}

// --- B76 §7.2, LIFTED (§9): an extern may RETURN a backed enum ---------------
//
// The deferral existed for one behavior — "confidently the wrong variant" — and
// §9's trap arm removes it rather than re-arguing it. The refusal that stood in
// the meantime is deleted, along with the hole B107 found in it: it worked by
// ENUMERATING the positions a host can construct a value from (return type,
// generic arguments, tuple elements, array elements) and had already missed
// one, a function-typed parameter's parameters. The trap asks nothing about
// provenance, so it has nothing to enumerate and nothing to miss.
//
// What is NOT claimed: the boundary still does not check. A host value outside
// the set enters unremarked, exactly as an `external fun f(): i32` answering
// `"hello"` does. `parse` is the non-panicking alternative and stays the shape
// to reach for when an out-of-set value is an expected input rather than a bug.

#[test]
fn b76_an_external_fun_can_return_a_backed_enum() {
    // The inversion of the refusal, at every shape it used to refuse: the bare
    // enum, the integer backing, and the nested forms its walker followed
    // (`Option<Align>`, `List<Align>`).
    assert_compiles(
        r#"
        enum Align { Start = "start", End = "end" }
        [extern("getAlign")]
        external fun get_align(): Align;
        fun main() { }
        "#,
    );
    assert_compiles(
        r#"
        enum Code { Ok = 200, NotFound = 404 }
        [extern("getCode")]
        external fun get_code(): Code;
        fun main() { }
        "#,
    );
    assert_compiles(
        r#"
        import std::option::Option;
        enum Align { Start = "start", End = "end" }
        [extern("getAlign")]
        external fun get_align(): Option<Align>;
        [extern("getAligns")]
        external fun get_aligns(): List<Align>;
        fun main() { }
        "#,
    );
    // A backed enum on an `external struct`'s method, the receiver form the
    // refusal also caught.
    assert_compiles(
        r#"
        enum Align { Start = "start", End = "end" }
        external struct Widget;
        impl Widget {
            [extern(method, "getAlign")]
            external fun align(self): Align;
        }
        fun main() { }
        "#,
    );
}

#[test]
fn b76_an_external_returned_backed_enum_round_trips_and_traps() {
    // The runtime proof, both directions in one program. `String` is the host
    // helper: `String(x)` is `x` for a string, so the extern hands vilan
    // whatever it is given — a legal value on the first call and `"middle"` on
    // the second, which is exactly what §7.2 said nothing could stop.
    //
    // Nothing stops it now either. What changed is what happens next: the value
    // reaches a `match` and is NAMED rather than becoming `Align::End`.
    let (stdout, stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        enum Align { Start = "flex-start", Center = "center", End = "flex-end" }
        [extern("String")]
        external fun host_align(text: str): Align;
        fun label(align: Align): str {
            match align { Align::Start => "s", Align::Center => "c", Align::End => "e" }
        }
        fun main() {
            let legal = host_align("flex-end");
            print(legal == Align::End);
            print(label(legal));
            print(label(host_align("middle")));
        }
        "#,
    );
    assert_eq!(
        stdout, "true\ne\n",
        "a legal host value should round-trip as its variant"
    );
    assert!(
        stderr.contains(r#"Align: "middle" is not one of its values"#),
        "an out-of-set host value should trap, got:\n{stderr}"
    );
    assert_ne!(code, 0, "a trapped program must not exit 0");
}

#[test]
fn b76_a_callback_parameter_is_covered_by_the_trap_not_by_enumeration() {
    // B107 / §9.2's P16, closed. `external fun on(handler: |Align| void)` is a
    // return position wearing a parameter's clothes — the HOST constructs the
    // value — and the refusal, which enumerated positions, compiled it clean
    // and let a host `handler("middle")` print `e` at exit 0.
    //
    // It still compiles clean; the refusal is gone. The value is caught one
    // step later, by the trap, which is the whole argument for (b) over (a):
    // the guard is at the `else`, so the position the value came in through
    // never had to be listed.
    let (stdout, stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        enum Align { Start = "flex-start", Center = "center", End = "flex-end" }
        [extern("Array.prototype.forEach.call")]
        external fun on_each_align(values: List<str>, handler: |Align| void): void;
        fun label(align: Align): str {
            match align { Align::Start => "s", Align::Center => "c", Align::End => "e" }
        }
        fun main() {
            on_each_align([ "flex-start" ], |a| print(label(a)));
            on_each_align([ "middle" ], |a| print(label(a)));
        }
        "#,
    );
    assert_eq!(
        stdout, "s\n",
        "the in-set callback value should label normally"
    );
    assert!(
        stderr.contains(r#"Align: "middle" is not one of its values"#),
        "the callback's out-of-set value should trap, got:\n{stderr}"
    );
    assert_ne!(code, 0, "a trapped program must not exit 0");
}

#[test]
fn b76_the_parse_path_is_the_non_panicking_alternative() {
    // What the deferral steered to is not retired by the lift — it is now the
    // CHOICE. Bind the backing type and `parse` when an out-of-set value is an
    // input you mean to handle; return the enum directly when it would be a
    // bug. Both compile, and this pins the difference at runtime: `parse`
    // answers `None` where the direct return would have trapped.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option;
        enum Align { Start = "flex-start", End = "flex-end" }
        [extern("String")]
        [doc(hidden)]
        external fun host_align_raw(text: str): str;
        fun host_align(text: str): Option<Align> { Align::parse(host_align_raw(text)) }
        fun main() {
            print(match host_align("flex-end") {
                Option::Some(let a) => a.value(),
                Option::None => "none",
            });
            print(match host_align("middle") {
                Option::Some(let a) => a.value(),
                Option::None => "none",
            });
        }
        "#,
        "flex-end\nnone\n",
    );
}

// --- B76 §4.2/§9.6: `json.vl`'s kind() family ---------------------------------
//
// §4.2's contingency, taken now that §7.2 has lifted. `kind()` returns the
// backed enum `JsonKind` and the four `is_*` predicates delete; §9.6's corrected
// shape is that the 13 in-file sites are `==` comparisons, NOT a match — so
// they pay nothing (`$a === "number"` either way), they gain no trap, and
// §4.2's "covered for free by exhaustiveness" was wrong about them.
//
// What the closed type buys is that `Object` and `Null` — the two members of
// the documented set that never got a predicate — are now as usable as the
// other four.

#[test]
fn b76_json_kind_is_a_backed_enum_carrying_the_intrinsic_strings() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::{ JsonKind, parse_json_value };
        fun main() {
            let value = parse_json_value("{\"n\":1,\"s\":\"x\",\"b\":true,\"a\":[],\"z\":null}");
            print(value.kind() == JsonKind::Object);
            print(value.field("n").kind() == JsonKind::Number);
            print(value.field("s").kind() == JsonKind::String);
            print(value.field("b").kind() == JsonKind::Bool);
            print(value.field("a").kind() == JsonKind::Array);
            // The two members that never got a predicate, now first-class.
            print(value.field("z").kind() == JsonKind::Null);
            print(JsonKind::Number.value());
        }
        "#,
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\nnumber\n",
    );
}

#[test]
fn b76_json_kind_comparisons_emit_what_the_predicates_compiled_to() {
    // §9.6's first claim, checked in bytes: the site is the same `===` against
    // the same literal `is_number()`'s body used to compile to, and the wrapper
    // is no longer emitted at all (emission is demand-driven, §8.2(a)). That is
    // why the rewrite is a net REDUCTION rather than a cost.
    let javascript = compile(
        r#"
        import std::print;
        import std::json::{ JsonKind, parse_json_value };
        fun main() { print(parse_json_value("1").kind() == JsonKind::Number); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains(r#"__json_kind("#),
        "the intrinsic should still be the whole of it, got:\n{javascript}"
    );
    assert!(
        javascript.contains(r#" === "number""#),
        "the comparison should be against the raw string, got:\n{javascript}"
    );
    assert!(
        !javascript.contains("function is_number"),
        "the deleted predicate should emit nothing, got:\n{javascript}"
    );
    assert!(
        !javascript.contains("__enum_trap"),
        "an `==` is not a match, so §9.6 pays no trap cost, got:\n{javascript}"
    );
}

#[test]
fn b76_a_json_kind_outside_the_set_compares_false_and_traps_in_a_match() {
    // The honest edge, recorded rather than papered over. A `JsonValue` is
    // whatever the host handed over, so `field()` on an absent key is
    // `undefined`, whose kind is none of the six. `==` answers `false` — the
    // behavior the `is_*` predicates had — and an exhaustive `match` over
    // `JsonKind` is the construct that says so instead of guessing, which is
    // exactly what §9's trap is for.
    let (stdout, stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::json::{ JsonKind, parse_json_value };
        fun main() {
            let absent = parse_json_value("{}").field("nope");
            print(absent.kind() == JsonKind::Null);
            print(absent.kind() == JsonKind::Object);
            print(match absent.kind() {
                JsonKind::Null => "null",
                JsonKind::Bool => "bool",
                JsonKind::Number => "number",
                JsonKind::String => "string",
                JsonKind::Array => "array",
                JsonKind::Object => "object",
            });
        }
        "#,
    );
    assert_eq!(
        stdout, "false\nfalse\n",
        "`==` should answer false for a kind outside the set"
    );
    assert!(
        stderr.contains(r#"JsonKind: "undefined" is not one of its values"#),
        "an exhaustive match should trap naming the raw kind, got:\n{stderr}"
    );
    assert_ne!(code, 0, "a trapped program must not exit 0");
}

// --- B76 §4.2: std's eleven CSS wrappers, deleted ----------------------------
//
// The payoff the survey measured: eleven of the fifteen payload-free enums in
// the whole standard library existed only to be converted to a host string,
// all in `std/src/style.vl`, 52 `match` arms that delete outright. The strings
// moved from the wrappers to the declarations, so the TYPE now says at its
// declaration what was only discoverable by reading a function 300 lines away.
//
// These pin the behavior the deletion has to preserve, at the shapes §2.1 calls
// out as the ones a name convention would have got wrong.

#[test]
fn b76_style_keyword_enums_carry_their_css_keywords() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ AlignItems, Display, JustifyContent, UserSelect };
        fun main() {
            // The five §2.1 names no case convention produces.
            print(AlignItems::Start.value());
            print(AlignItems::End.value());
            print(JustifyContent::Between.value());
            print(Display::Hidden.value());
            print(UserSelect::Off.value());
        }
        "#,
        "flex-start\nflex-end\nspace-between\nnone\nnone\n",
    );
}

#[test]
fn b76_style_wrappers_still_write_the_same_declaration() {
    // The wrappers are one line now (`self.raw("display", value.value())`), and
    // what they write must not have moved. A class name is a content hash of
    // `key|declaration`, so these two are the declarations themselves: write
    // `"inline_block"` instead of `"inline-block"` and both change.
    // (`vilan/test/style.css` pins the declaration text itself, byte for byte.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ AlignItems, Display, RadialExtent, Style, style };
        fun main() {
            let card = const style().display(Display::InlineBlock).align_items(AlignItems::End);
            print(card.class_list());
            print(RadialExtent::ClosestCorner.value());
        }
        "#,
        "sfatq7m s1g8z7cm\nclosest-corner\n",
    );
}

// A RESOURCE backed enum: the conversions used to be synthesized for it, and
// `fun value(self)` reads a resource out of a loan — so the declaration itself
// failed, with an error about a body the author never wrote. A resource's
// identity is not its copyable backing value (the rule `check_hashable_boundary`
// already states for a resource field), so it is offered neither conversions nor
// `Hashable`.

#[test]
fn a_resource_backed_enum_declares_without_synthesized_conversions() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource enum Handle { Open = 1, Closed = 2 }
        fun main() {
            let handle = Handle::Open;
            print(handle == Handle::Open);
        }
        "#,
        "true\n",
    );
}

#[test]
fn a_resource_backed_enum_has_no_value_member() {
    assert_fails_with(
        r#"
        import std::print;
        resource enum Handle { Open = 1, Closed = 2 }
        fun main() { print(Handle::Open.value()); }
        "#,
        "no method 'value'",
    );
}

#[test]
fn a_resource_backed_enum_is_not_hashable() {
    // The resource rule wins over the bare-lowering one: a resource cannot be
    // hashed by value, so it is not a key however it lowers.
    assert_fails_with(
        r#"
        import std::print;
        import std::set::Set;
        resource enum Handle { Open = 1, Closed = 2 }
        fun main() {
            mut seen: Set<Handle> = Set::new();
            seen.insert(Handle::Open);
            print(seen.len());
        }
        "#,
        "does not implement trait 'Hashable'",
    );
}

// B117 — §10.5's rule at the DERIVE. A resource is an owned handle, not plain
// data: `Wire::rebuild` and `FromJson::from_json` both MINT a `Self` out of
// bytes, which for a resource is a twin nothing owns. The subject of the derive
// was the one position nothing tested — `check_wire_boundary` had covered the
// resource FIELD since destruction.md §8 — so all four shapes (backed/plain enum
// × `Wire`/`Json`, and the struct twin) reached the generators. The enum shapes
// then failed inside `match self` with "cannot move the resource `self` out of
// this function", an error about a body the author never wrote; the STRUCT
// shapes were worse, compiling clean and giving a resource a serialized form.

#[test]
fn b117_wire_is_refused_for_a_backed_resource_enum() {
    assert_fails_spanning(
        r#"
        [derive(Wire)]
        resource enum Handle { Open = 1, Closed = 2 }
        fun main() {}
        "#,
        "Wire",
        "`Wire` cannot be derived for the resource enum `Handle`",
    );
}

#[test]
fn b117_wire_is_refused_for_a_plain_resource_enum() {
    // The plain shape reaches a different generator (externally tagged, no
    // `value()`), and the refusal is above both.
    assert_fails_with(
        r#"
        [derive(Wire)]
        resource enum Handle { Open, Closed }
        fun main() {}
        "#,
        "`Wire` cannot be derived for the resource enum `Handle`",
    );
}

#[test]
fn b117_json_is_refused_for_a_backed_resource_enum() {
    assert_fails_with(
        r#"
        [derive(Json)]
        resource enum Handle { Open = 1, Closed = 2 }
        fun main() {}
        "#,
        "`Json` cannot be derived for the resource enum `Handle`",
    );
}

#[test]
fn b117_json_is_refused_for_a_plain_resource_enum() {
    assert_fails_with(
        r#"
        [derive(Json)]
        resource enum Handle { Open, Closed }
        fun main() {}
        "#,
        "`Json` cannot be derived for the resource enum `Handle`",
    );
}

#[test]
fn b117_wire_is_refused_for_a_resource_struct() {
    // The struct twin USED TO COMPILE: a struct's `describe` reads its fields
    // through the loan, so nothing moved and nothing complained — the derive
    // silently handed a resource a wire format.
    assert_fails_spanning(
        r#"
        [derive(Wire)]
        resource struct Conn { id: i32 }
        fun main() {}
        "#,
        "Wire",
        "`Wire` cannot be derived for the resource struct `Conn`",
    );
}

#[test]
fn b117_json_is_refused_for_a_resource_struct() {
    assert_fails_with(
        r#"
        [derive(Json)]
        resource struct Conn { id: i32 }
        fun main() {}
        "#,
        "`Json` cannot be derived for the resource struct `Conn`",
    );
}

#[test]
fn b117_the_refusal_replaces_the_generated_code_error() {
    // The point of refusing at the derive rather than after expansion: nothing
    // is generated, so the author never meets a diagnostic about a body they
    // did not write.
    assert_fails_without(
        r#"
        [derive(Wire)]
        resource enum Handle { Open = 1, Closed = 2 }
        fun main() {}
        "#,
        "in code generated by this attribute",
    );
}

#[test]
fn b117_a_refused_wire_derive_leaves_the_resource_not_wire() {
    // A refused derive must not register the name as Wire. `[rpc]`'s signature
    // check reads `wire_names` and has no resource guard of its own — the
    // resource-FIELD rejects do, which is why this is the shape that shows it —
    // so a refused `Conn` left in the set would still cross the wire, and the
    // refusal would be advice rather than a rule.
    assert_fails_with(
        r#"
        [derive(Wire)]
        resource struct Conn { id: i32 }
        struct Pool {}
        impl Pool {
            [rpc] fun adopt(self, conn: Conn): i32 { 0 }
        }
        fun main() {}
        "#,
        "of `[rpc]` method `adopt` is `Conn`, which is not Wire",
    );
}

#[test]
fn b117_a_refused_wire_derive_still_meets_the_resource_field_reject() {
    // The other reader of `wire_names`: a plain-data type holding the refused
    // resource is rejected at the field, by the resource arm that precedes the
    // not-Wire one.
    assert_fails_with(
        r#"
        [derive(Wire)]
        resource struct Conn { id: i32 }
        [derive(Wire)]
        struct Envelope { conn: Conn }
        fun main() {}
        "#,
        "field `conn` of `[derive(Wire)]` type `Envelope` is the resource `Conn`",
    );
}

#[test]
fn b117_the_other_derives_on_a_resource_survive_the_refusal() {
    // Only `Wire`/`Json` are refused, and per derive NAME: `PartialEq`/`Debug`
    // on a resource struct read their fields through the loan and stay legal
    // (`resource_struct_carries_a_derive_through_expansion`).
    assert_fails_once_with(
        r#"
        import std::print;
        [derive(PartialEq, Wire, Debug)]
        resource struct Session { id: i32, name: str }
        fun main() {
            let session = Session { id = 1, name = "a" };
            print(session.debug());
        }
        "#,
        "cannot be derived for the resource struct `Session`",
    );
}

#[test]
fn b117_a_resource_by_containment_still_names_its_field() {
    // Keyed on the DECLARED `resource` modifier: a type that is a resource by
    // containment has its root cause in the field, which `check_wire_boundary`
    // already names — one mistake, one message, not two.
    assert_fails_without(
        r#"
        resource struct Db { handle: i32 }
        [derive(Wire)]
        struct Envelope { db: Db }
        fun main() {}
        "#,
        "cannot be derived for the resource",
    );
}

#[test]
fn b117_a_plain_data_wire_derive_is_untouched() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::encode_json;
        [derive(Wire)]
        struct Note { id: i32, text: str }
        fun main() {
            print(encode_json(Note { id = 7, text = "hi" }));
        }
        "#,
        "{\"id\":7,\"text\":\"hi\"}\n",
    );
}

// B120 — §10.8's "left open, filed not fixed": `Json` had no boundary check
// at all, so a resource FIELD inside a `[derive(Json)]` plain-data struct
// reached the generated-code error class §10.8 exists to remove ("`Db` has
// no method `to_json`"). `check_json_boundary` is `check_partialeq_boundary`'s
// twin, not `check_wire_boundary`'s: Json's codegen reads `to_json`/
// `from_json_value` straight off each field's own type, so there is no
// all-fields type DOMAIN to police — only the resource-field reject.

#[test]
fn b120_derive_json_rejects_a_resource_field() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        [derive(Json)]
        struct Envelope { db: Db }
        fun main() {}
        "#,
        "field `db` of `[derive(Json)]` type `Envelope` is the resource `Db`",
    );
}

#[test]
fn b120_derive_json_rejects_a_nested_resource_field() {
    // Containment two levels deep: `Holder` is a resource by CONTAINING `Db`
    // (no `resource` modifier of its own), and `Envelope`'s check names ITS
    // OWN field (`holder`) and its immediate type (`Holder`) — the root cause
    // of `Envelope` not being serializable, without walking further down to
    // blame `Db` by name (that is `Holder`'s own business, if it ever derives
    // anything).
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        struct Holder { db: Db }
        [derive(Json)]
        struct Envelope { holder: Holder }
        fun main() {}
        "#,
        "field `holder` of `[derive(Json)]` type `Envelope` is the resource `Holder`",
    );
}

#[test]
fn b120_derive_json_rejects_a_resource_enum_payload() {
    // The enum shape: a resource PAYLOAD, not a struct field — the same
    // `collect_derived_members` enum arm Wire's check already reuses.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        [derive(Json)]
        enum Wrapper { Holds(Db), Empty }
        fun main() {}
        "#,
        "variant `Holds` payload 0 of `[derive(Json)]` type `Wrapper` is the resource `Db`",
    );
}

#[test]
fn b120_a_json_derived_struct_with_no_resource_stays_legal() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::Json;
        [derive(Json)]
        struct Note { id: i32, text: str }
        fun main() {
            print(Note { id = 7, text = "hi" }.to_json());
        }
        "#,
        "{\"id\":7,\"text\":\"hi\"}\n",
    );
}

#[test]
fn b120_the_resource_field_reject_and_the_declared_modifier_refusal_compose() {
    // §10.8's declared-modifier refusal (the DERIVE's subject) and B120's
    // field check (destruction.md §8's shape, now Json's too) are two
    // different rules answering two different questions — verified to
    // compose the way Wire's family already does: each fires ALONE where
    // only one applies...
    assert_fails_with(
        r#"
        [derive(Json)]
        resource struct Conn { id: i32 }
        fun main() {}
        "#,
        "`Json` cannot be derived for the resource struct `Conn`",
    );
    // ...and, driven at the OTHER type — a plain-data struct holding that
    // same refused resource by field — the field check fires on its own,
    // naming the field, exactly as Wire's `b117_a_refused_wire_derive_still_meets_the_resource_field_reject`
    // does.
    assert_fails_with(
        r#"
        [derive(Json)]
        resource struct Conn { id: i32 }
        [derive(Json)]
        struct Envelope { conn: Conn }
        fun main() {}
        "#,
        "field `conn` of `[derive(Json)]` type `Envelope` is the resource `Conn`",
    );
}

#[test]
fn b120_json_has_no_rpc_escape_to_close() {
    // §10.8's collector-skip closure exists because `wire_names` has a SECOND
    // reader (`[rpc]`'s signature check) that would trust a refused type back
    // onto the wire if the name were left registered. `check_json_boundary`
    // builds no name set at all — there is nothing analogous for a `[rpc]`
    // check to consult, so there is no escape to close: a `[derive(Json)]`
    // resource struct used as an `[rpc]` parameter is rejected for the
    // ordinary, pre-existing reason (it was never `Wire`, which is the only
    // thing `[rpc]` signatures require), unaffected by this file's checks.
    assert_fails_with(
        r#"
        [derive(Json)]
        resource struct Conn { id: i32 }
        struct Pool {}
        impl Pool {
            [rpc] fun adopt(self, conn: Conn): i32 { 0 }
        }
        fun main() {}
        "#,
        "of `[rpc]` method `adopt` is `Conn`, which is not Wire",
    );
}

#[test]
fn b120_a_resource_by_containment_still_names_only_its_field() {
    // Keyed on the DECLARED `resource` modifier (§10.8): a type that is a
    // resource by CONTAINMENT (no modifier of its own) gets exactly ONE
    // message — the field check's — never the subject-level refusal too.
    assert_fails_without(
        r#"
        resource struct Db { handle: i32 }
        [derive(Json)]
        struct Envelope { db: Db }
        fun main() {}
        "#,
        "cannot be derived for the resource",
    );
}

// §7.1/§8.5: a bare-lowered enum IS its backing value at runtime — a plain JS
// number or string, which `canonical_hash` returns unchanged — so the backing
// value is the key and `Map<Align, V>` / `Set<Align>` work without a derive.
// The impl is synthesized beside `value()`/`parse()`, off the same opt-in.

#[test]
fn b76_a_string_backed_enum_keys_a_map() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        enum Align { Start = "flex-start", End = "flex-end" }
        fun main() {
            mut widths: Map<Align, i32> = Map::new();
            widths.insert(Align::Start, 1);
            widths.insert(Align::End, 2);
            print(widths.get(Align::Start).unwrap_or(0));
            print(widths.get(Align::End).unwrap_or(0));
            print(widths.contains_key(Align::Start));
            print(widths.len());
            widths.remove(Align::Start);
            print(widths.contains_key(Align::Start));
        }
        "#,
        "1\n2\ntrue\n2\nfalse\n",
    );
}

#[test]
fn b76_an_integer_backed_enum_keys_a_set() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        enum Level { Low = 0, High = 1 }
        fun main() {
            mut seen: Set<Level> = Set::new();
            seen.insert(Level::High);
            print(seen.contains(Level::High));
            print(seen.contains(Level::Low));
            // A second insert of the same variant is the same key.
            seen.insert(Level::High);
            print(seen.len());
        }
        "#,
        "true\nfalse\n1\n",
    );
}

#[test]
fn b76_an_auto_incremented_backing_keys_a_set() {
    // `Hashable` keys off the LOWERING rule (payload-free plus one explicit
    // value), not off the stricter "every variant carries a written literal"
    // rule `value()`/`parse()` need — so the C-style tail comes along.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        enum Walked { A = 5, B, C }
        fun main() {
            mut seen: Set<Walked> = Set::new();
            seen.insert(Walked::B);
            print(seen.contains(Walked::B));
            print(seen.contains(Walked::C));
            print(seen.len());
        }
        "#,
        "true\nfalse\n1\n",
    );
}

#[test]
fn b76_a_backed_enums_hash_is_its_backing_values_hash() {
    // Coherence, observed the way a user-built container observes it (a
    // `Map<Hash, ..>`, which is what `collections.md` documents): one variant is
    // one key however the hash is re-derived, two variants are two keys, and the
    // enum's key is its BACKING VALUE's key — the identity the whole design
    // rests on.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::{ Hash, Hashable };
        enum Align { Start = "flex-start", End = "flex-end" }
        fun main() {
            mut by_hash: Map<Hash, i32> = Map::new();
            by_hash.insert(Align::Start.hash(), 10);
            by_hash.insert(Align::End.hash(), 20);
            print(by_hash.len());
            print(by_hash.get(Align::Start.hash()).unwrap_or(0));
            // Re-derived from a second mention of the same variant: same key.
            by_hash.insert(Align::Start.hash(), 11);
            print(by_hash.len());
            // The backing value, and the raw string it is, hash to that key too.
            print(by_hash.get(Align::Start.value().hash()).unwrap_or(0));
            print(by_hash.get("flex-start".hash()).unwrap_or(0));
        }
        "#,
        "2\n10\n2\n11\n11\n",
    );
}

#[test]
fn b76_an_integer_backed_enums_hash_is_its_numbers_hash() {
    // The other backing, same identity — and the cross-check that a backed
    // enum does NOT collide with an unrelated key of the other shape.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::{ Hash, Hashable };
        enum Level { Low = 0, High = 1 }
        fun main() {
            mut by_hash: Map<Hash, str> = Map::new();
            by_hash.insert(Level::High.hash(), "high");
            print(by_hash.get(1.hash()).unwrap_or("miss"));
            print(by_hash.get("1".hash()).unwrap_or("miss"));
        }
        "#,
        "high\nmiss\n",
    );
}

#[test]
fn b76_std_ordering_keys_a_map() {
    // The backed enum std already shipped, now a key without ceremony.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::compare::Ordering;
        fun main() {
            mut labels: Map<Ordering, str> = Map::new();
            labels.insert(Ordering::Less, "lt");
            labels.insert(Ordering::Greater, "gt");
            print(labels.get(Ordering::Less).unwrap_or("?"));
            print(labels.len());
        }
        "#,
        "lt\n2\n",
    );
}

#[test]
fn b76_a_backed_enum_key_emits_its_bare_value() {
    // Nothing wraps the key on the way into the map: the emitted program hands
    // the native string straight to `insert`, exactly as §1.3's host-boundary
    // probe found for a call.
    let javascript = compile(
        r#"
        import std::print;
        import std::map::Map;
        enum Align { Start = "flex-start", End = "flex-end" }
        fun main() {
            mut widths: Map<Align, i32> = Map::new();
            widths.insert(Align::Start, 1);
            print(widths.len());
        }
        "#,
    )
    .expect("expected a clean compile");
    assert!(javascript.contains(r#""flex-start""#), "{javascript}");
    assert!(!javascript.contains("[ 0 ]"), "{javascript}");
}

#[test]
fn b76_a_plain_enum_is_still_not_hashable() {
    // The §3.1(b) conjunction decides this too: `enum Plain { A, B }` keeps its
    // `[0]`/`[1]` ARRAY form, so it is an aggregate with no bare value to key
    // by — exactly the reference-identity footgun `Hashable` exists to stop.
    // It needs `[derive(Hashable)]` like any other aggregate.
    assert_fails_with(
        r#"
        import std::print;
        import std::set::Set;
        enum Plain { A, B, C }
        fun main() {
            mut seen: Set<Plain> = Set::new();
            seen.insert(Plain::B);
            print(seen.len());
        }
        "#,
        "'Plain' does not implement trait 'Hashable'",
    );
}

#[test]
fn b76_a_plain_enum_keys_a_set_with_the_derive() {
    // The escape hatch the previous pin points at, and it works by value.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        [derive(Hashable)]
        enum Plain { A, B, C }
        fun main() {
            mut seen: Set<Plain> = Set::new();
            seen.insert(Plain::B);
            print(seen.contains(Plain::B));
            print(seen.contains(Plain::C));
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn b76_a_payload_enum_is_still_not_hashable() {
    // The tagged-array lowering is out of scope: a payload enum gets no
    // synthesized `Hashable` and is refused at the key, with the ordinary bound
    // diagnostic rather than a runtime surprise.
    assert_fails_with(
        r#"
        import std::print;
        import std::set::Set;
        enum Payload { Num(i32), Text(str) }
        fun main() {
            mut seen: Set<Payload> = Set::new();
            seen.insert(Payload::Num(3));
            print(seen.len());
        }
        "#,
        "'Payload' does not implement trait 'Hashable'",
    );
}

#[test]
fn b76_a_payload_enum_keys_a_set_with_the_derive() {
    // Its route is unchanged by this arc: the derive, with its all-fields gate.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        [derive(Hashable)]
        enum Payload { Num(i32), Text(str) }
        fun main() {
            mut seen: Set<Payload> = Set::new();
            seen.insert(Payload::Num(3));
            print(seen.contains(Payload::Num(3)));
            print(seen.contains(Payload::Num(4)));
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn b76_a_backed_enum_field_of_a_derived_hashable_type_is_accepted() {
    // The two oracles have to agree: the impl table says `Align` is a key, so
    // the derive's syntactic all-fields check must not reject it as a FIELD.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        enum Align { Start = "flex-start", End = "flex-end" }
        [derive(Hashable)]
        struct Slot { align: Align, index: i32 }
        fun main() {
            mut seen: Set<Slot> = Set::new();
            seen.insert(Slot { align = Align::Start, index = 1 });
            print(seen.contains(Slot { align = Align::Start, index = 1 }));
            print(seen.contains(Slot { align = Align::End, index = 1 }));
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn b76_a_plain_enum_field_of_a_derived_hashable_type_still_needs_its_own_derive() {
    // The contrast that keeps the previous pin honest: the field rule tracks
    // the lowering, not "is an enum".
    assert_fails_with(
        r#"
        import std::print;
        import std::hash::Hashable;
        enum Plain { A, B }
        [derive(Hashable)]
        struct Slot { plain: Plain }
        fun main() { print(1); }
        "#,
        "field `plain` of `[derive(Hashable)]` type `Slot` is `Plain`",
    );
}

#[test]
fn b76_a_redundant_hashable_derive_on_a_backed_enum_is_a_no_op() {
    // The derive and the synthesis emit the identical `canonical_hash(self)`
    // body, so there is nothing for a duplicate-impl error to protect — and a
    // program written before the impl was synthesized keeps compiling.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::set::Set;
        import std::hash::Hashable;
        [derive(Hashable)]
        enum Align { Start = "flex-start", End = "flex-end" }
        fun main() {
            mut seen: Set<Align> = Set::new();
            seen.insert(Align::Start);
            print(seen.contains(Align::Start));
        }
        "#,
        "true\n",
    );
}

#[test]
fn b76_a_hand_written_hashable_impl_on_a_backed_enum_collides() {
    // A hand-written impl may mean something else, and which one wins is B73's
    // open specificity question — so it stays a duplicate, reported AT the
    // author's impl with a note saying the other one is the compiler's.
    assert_fails_with(
        r#"
        import std::print;
        import std::hash::{ Hashable, Hash, canonical_hash };
        enum Align { Start = "flex-start", End = "flex-end" }
        impl Align with Hashable {
            fun hash(self): Hash { canonical_hash(self) }
        }
        fun main() { print(1); }
        "#,
        "'Hashable' is already implemented for 'Align'",
    );
}

#[test]
fn b76_a_generic_backed_enum_gets_no_hashable() {
    // §8.2(d)'s rule, unchanged: a generic enum gets no synthesized members at
    // all, `Hashable` included.
    assert_fails_with(
        r#"
        import std::print;
        import std::set::Set;
        enum Phantom<T> { A = 1, B = 2 }
        fun main() {
            mut seen: Set<Phantom<i32>> = Set::new();
            seen.insert(Phantom::A);
            print(seen.len());
        }
        "#,
        "does not implement trait 'Hashable'",
    );
}

// B105 (a compound assignment evaluated an impure subscript twice). `x op= v`
// desugars to `x = x op v`, which walks the TARGET PLACE twice — so every
// effectful subscript in it ran twice: `ys[bump()] += 1` emitted
// `__at_put(ys, bump(), __at(ys, bump()) + 1)`. Each is now evaluated once,
// into a temp both walks name. A PURE subscript is left alone.
// ---------------------------------------------------------------------------

#[test]
fn a_compound_assignment_evaluates_an_impure_index_once() {
    // The filed repro, counted: one call, and the increment lands once.
    assert_compiles_and_runs(
        r#"
        import std::print;
        mut calls = 0;
        fun bump(): i32 {
            calls = calls + 1;
            0
        }
        fun main() {
            mut ys = [10, 20];
            ys[bump()] += 1;
            print(ys[0]);
            print(calls);
        }
        "#,
        "11\n1\n",
    );
}

#[test]
fn a_compound_assignment_evaluates_the_index_before_the_value() {
    // Source order: the subscript, then the read, then the right-hand side.
    // The un-hoisted emission ran them in that order too (a JS call evaluates
    // its arguments left to right), so the temp must not reorder anything.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun index(): i32 { print("index"); 0 }
        fun amount(): i32 { print("amount"); 5 }
        fun main() {
            mut ys = [10, 20];
            ys[index()] += amount();
            print(ys[0]);
        }
        "#,
        "index\namount\n15\n",
    );
}

#[test]
fn a_compound_assignment_with_a_pure_index_mints_no_temp() {
    // The other direction, in bytes: evaluating `index` twice is not an
    // observable difference, so the emission is exactly what it was. Hoisting
    // unconditionally is *correct* and was measured — it moves this shape's
    // golden and buys nothing (proposal/transparent-references.md).
    let source = r#"
        import std::print;
        fun main() {
            mut ys = [10, 20];
            let index = 1;
            ys[index] += 5;
            print(ys[1]);
        }
        "#;
    match compile(source) {
        Ok(js) => assert!(
            js.contains("__at_put(ys, index, __at(ys, index) + 5)"),
            "a pure subscript was hoisted into a temp:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
    assert_compiles_and_runs(source, "25\n");
}

#[test]
fn a_compound_assignment_hoists_an_index_in_the_targets_subject() {
    // The subscript is not at the top of the target place — `cells[bump()].n`
    // is a FIELD of an indexed element — so the hoist walks the whole spine.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Cell { n: i32 }
        mut calls = 0;
        fun bump(): i32 {
            calls = calls + 1;
            0
        }
        fun main() {
            mut cells = [Cell { n = 10 }, Cell { n = 20 }];
            cells[bump()].n += 1;
            print(cells[0].n);
            print(calls);
        }
        "#,
        "11\n1\n",
    );
}

#[test]
fn a_compound_assignment_hoists_every_index_of_a_nested_target() {
    // Two subscripts, each effectful and each its own temp, minted root-first
    // so the calls run in source order.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun row(): i32 { print("row"); 0 }
        fun column(): i32 { print("column"); 1 }
        fun main() {
            mut grid = [[1, 2], [3, 4]];
            grid[row()][column()] += 100;
            print(grid[0][1]);
        }
        "#,
        "row\ncolumn\n102\n",
    );
}

#[test]
fn a_compound_assignment_through_a_view_hoists_its_index() {
    // R5 wraps a view target and its synthesized re-read alike in a `Dereference`,
    // so the analyzer's compound mark sits one node down. The hoist looks under it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        mut calls = 0;
        fun bump(): i32 {
            calls = calls + 1;
            0
        }
        fun add(v: &mut List<i32>) {
            v[bump()] += 1;
        }
        fun main() {
            mut ys = [10, 20];
            add(&mut ys);
            print(ys[0]);
            print(calls);
        }
        "#,
        "11\n1\n",
    );
}

#[test]
fn a_plain_indexed_assignment_still_evaluates_its_index_once() {
    // The neighbour that was always right: a non-compound write walks the
    // target once, and the hoist must not touch it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        mut calls = 0;
        fun bump(): i32 {
            calls = calls + 1;
            0
        }
        fun main() {
            mut ys = [10, 20];
            ys[bump()] = 99;
            print(ys[0]);
            print(calls);
        }
        "#,
        "99\n1\n",
    );
}

#[test]
fn two_hand_written_subscripts_are_not_collapsed_into_one() {
    // Why the compound-ness comes from the analyzer's record and not from the
    // shape: `ys[first()] = ys[second()] + 1` looks exactly like the desugared
    // form and means something else entirely. Both calls run, at their own
    // indices.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun first(): i32 { print("first"); 0 }
        fun second(): i32 { print("second"); 1 }
        fun main() {
            mut ys = [10, 20];
            ys[first()] = ys[second()] + 1;
            print(ys[0]);
            print(ys[1]);
        }
        "#,
        "first\nsecond\n21\n20\n",
    );
}

// ---------------------------------------------------------------------------
// B106 (a discriminant past 2^53 is unrepresentable in the emission). A backed
// enum IS its backing value at runtime, emitted as a bare JS numeric literal —
// and a double holds integers exactly only to 2^53 - 1, so
// `= 9007199254740993` emitted a literal the host reads back as `…992`.
// Self-consistent in-tree (every site emitted the same wrong number) and wrong
// across a host boundary. The range check lands in B79's validation family, at
// i53's edge, and the diagnostic names the emission target as the reason.
// ---------------------------------------------------------------------------

#[test]
fn b106_the_i53_edge_is_a_legal_discriminant() {
    // 2^53 - 1: the largest integer a JS number holds exactly, and the last one
    // that round-trips through the emitted literal.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Edge { Zero = 0, Max = 9007199254740991 }
        fun main() {
            print(match Edge::Max { Edge::Zero => "zero", Edge::Max => "max" });
        }
        "#,
        "max\n",
    );
}

#[test]
fn b106_one_past_the_i53_edge_is_refused() {
    // One more, and the emitted literal is a different number than the source
    // wrote. The diagnostic states the bound AND why it is that bound.
    assert_fails_with(
        r#"
        enum Edge { Zero = 0, Over = 9007199254740992 }
        fun main() { }
        "#,
        "the enum discriminant `9007199254740992` is out of range \
         (-9007199254740991 ..= 9007199254740991): a backed enum is a JS number \
         at runtime, and an integer past 2^53 - 1 has no exact double, so the \
         emitted literal would be a different value",
    );
}

#[test]
fn b106_the_negative_i53_edge_is_legal_and_one_past_it_is_refused() {
    // The negative twin, and the change B106 makes to B79's shape: the bound is
    // SYMMETRIC. `i64`'s negative end reached one further because two's
    // complement does; a JS number has no such asymmetry, so `-9007199254740991`
    // is the last legal value in that direction too.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Edge { Zero = 0, Min = -9007199254740991 }
        fun main() {
            print(match Edge::Min { Edge::Zero => "zero", Edge::Min => "min" });
        }
        "#,
        "min\n",
    );
    assert_fails_with(
        r#"
        enum Edge { Zero = 0, Under = -9007199254740992 }
        fun main() { }
        "#,
        "the enum discriminant `-9007199254740992` is out of range",
    );
}

#[test]
fn b106_the_old_i64_bound_is_now_refused() {
    // The value B79 accepted and this fix does not — the one that made the
    // range a lie: `i64::MAX` cannot be an emitted JS literal at all.
    assert_fails_with(
        r#"
        enum Edge { Zero = 0, Max = 9223372036854775807 }
        fun main() { }
        "#,
        "the enum discriminant `9223372036854775807` is out of range",
    );
}

#[test]
fn b106_a_continued_discriminant_stops_at_the_same_edge() {
    // A continued value is emitted as the same bare literal, so the sequence
    // must stop where an explicit discriminant does — one rule, not two.
    assert_fails_with(
        r#"
        enum Walk { A = 9007199254740990, B, C }
        fun main() { }
        "#,
        "variant 'C' continues the discriminant sequence past 9007199254740991, \
         the largest integer a JS number holds exactly",
    );
}

#[test]
fn b106_a_hex_discriminant_is_range_checked_too() {
    // The range check always spoke radix 16 (B79's fix); the narrower bound
    // inherits that, so hex cannot smuggle a value past it.
    assert_fails_with(
        r#"
        enum Mask { Zero = 0, Wide = 0x20000000000000 }
        fun main() { }
        "#,
        "the enum discriminant `0x20000000000000` is out of range",
    );
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Mask { Zero = 0, Edge = 0x1FFFFFFFFFFFFF }
        fun main() {
            print(match Mask::Edge { Mask::Zero => "zero", Mask::Edge => "edge" });
        }
        "#,
        "edge\n",
    );
}

// ---------------------------------------------------------------------------
// B111 (a walked discriminant gets `value()`/`parse()`). `enum Walked { A = 5,
// B, C }` lowers to the bare numbers 5/6/7 — the walk resolves every variant by
// continuing the C-style sequence — but the generators read the declaration
// separately and bailed on the first variant with no WRITTEN literal, so the
// enum got no `value()`, no `parse()`, and the name-tagged JSON shape of an
// unbacked enum. The fix is one reader (`read_enum_backing`, backed-enums.md
// §10) shared by the walk, the generators, and §3.7's duplicate check, so a
// variant's effective backing is computed once.
// ---------------------------------------------------------------------------

#[test]
fn b111_a_walked_discriminant_gets_value_and_parse() {
    // The headline: `B` wrote nothing, is 6 at runtime, and now says so. The
    // round trip goes out through `value()` and back through `parse()`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };

        enum Walked { A = 5, B, C }

        fun main() {
            print(Walked::A.value());  // 5
            print(Walked::B.value());  // 6 — the walked one
            print(Walked::C.value());  // 7
            print(match Walked::parse(6) {
                Some(let variant) => variant.value(),
                None => -1,
            });                        // 6
            print(match Walked::parse(99) {
                Some(let variant) => variant.value(),
                None => -1,
            });                        // -1
        }
        "#,
        "5\n6\n7\n6\n-1\n",
    );
}

#[test]
fn b111_a_walk_from_the_implicit_zero_gets_them_too() {
    // The `enum Level { Low = 0, Mid, High }` shape §3.1(b) names: one explicit
    // value converts the whole declaration, so the other two walk from it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };

        enum Level { Low = 0, Mid, High }

        fun main() {
            print(Level::High.value());  // 2
            print(match Level::parse(1) {
                Some(let variant) => variant.value(),
                None => -1,
            });                          // 1
        }
        "#,
        "2\n1\n",
    );
}

#[test]
fn b111_a_negative_walk_gets_them_too() {
    // The sequence continues from a negative value the same way, and `parse`'s
    // `if`/`else if` chain (not a `match`) is what lets a negative literal be
    // compared at all — the reason §3.8 chose that shape.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };

        enum Signed { Less = -1, Equal, Greater }

        fun main() {
            print(Signed::Less.value());     // -1
            print(Signed::Equal.value());    // 0
            print(Signed::Greater.value());  // 1
            print(match Signed::parse(0) {
                Some(let variant) => variant.value(),
                None => -99,
            });                              // 0
        }
        "#,
        "-1\n0\n1\n0\n",
    );
}

#[test]
fn b111_a_written_literal_keeps_its_own_spelling() {
    // The unified reader resolves values, but the generator still reprints the
    // literal the author WROTE where there is one: hex stays hex in the
    // generated `parse` chain. Only the walked variant, which wrote nothing, is
    // rendered from the resolved value.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };

        enum Hexed { A = 0x10, B, C = 0x20 }

        fun main() {
            print(Hexed::A.value());  // 16
            print(Hexed::B.value());  // 17
            print(Hexed::C.value());  // 32
            print(match Hexed::parse(17) {
                Some(let variant) => variant.value(),
                None => -1,
            });                       // 17
        }
        "#,
        "16\n17\n32\n17\n",
    );
}

#[test]
fn b111_a_walked_collision_is_still_rejected() {
    // §3.7's uniqueness rule already counted walked values, and it still does:
    // `B` walks onto 6 and `C` writes 6. Unifying the readers must not make the
    // generator's view of the enum the one validation trusts.
    assert_fails_with(
        r#"
        enum Walked { A = 5, B, C = 6 }
        fun main() { }
        "#,
        "variant 'C' has discriminant 6, which 'B' already uses; two variants of \
         'Walked' cannot share one",
    );
}

#[test]
fn b111_a_broken_walk_generates_no_conversions() {
    // The other half of the collision rule. A generator emits vilan SOURCE, so
    // it stays silent on a declaration the walk already rejects — otherwise the
    // generated `parse` chain would compare against a value the compiler does
    // not believe in and report the enum a second time, from inside code the
    // author never wrote. The cost is the `has no method` at the call site,
    // which is what a broken enum has always produced.
    assert_fails_with(
        r#"
        import std::print;
        enum Walked { A = 5, B, C = 6 }
        fun main() { print(Walked::A.value()); }
        "#,
        "Walked has no method 'value'",
    );
}

#[test]
fn b111_a_string_backing_still_needs_a_literal_per_variant() {
    // §3.1(a) is untouched by the unification: there is no successor of
    // `"start"`, so the walk has nothing to continue and the missing value is a
    // hard error rather than a silently synthesized one.
    assert_fails_with(
        r#"
        enum Align { Start = "flex-start", End }
        fun main() { }
        "#,
        "variant 'End' has no backing value, and a string backing has no successor \
         to continue from; give every variant of 'Align' its own string",
    );
}

#[test]
fn b111_a_fully_written_string_enum_is_unchanged() {
    // The other half of the same claim: a string-backed enum still gets its
    // conversions, and they still carry the author's own text.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };

        enum Align { Start = "flex-start", End = "flex-end" }

        fun main() {
            print(Align::Start.value());  // flex-start
            print(match Align::parse("flex-end") {
                Some(let variant) => variant.value(),
                None => "miss",
            });                           // flex-end
            print(match Align::parse("center") {
                Some(let variant) => variant.value(),
                None => "miss",
            });                           // miss
        }
        "#,
        "flex-start\nflex-end\nmiss\n",
    );
}

#[test]
fn b111_the_i53_continuation_edge_still_holds_through_the_unified_reader() {
    // B106's edge is enforced inside the one reader now, so it has to hold for
    // every consumer at once: the sequence stops where an explicit discriminant
    // does, and a walk that runs off the end is refused rather than generating
    // conversions over a value the emission cannot carry.
    assert_fails_with(
        r#"
        enum Walk { A = 9007199254740990, B, C }
        fun main() { }
        "#,
        "variant 'C' continues the discriminant sequence past 9007199254740991, \
         the largest integer a JS number holds exactly",
    );
    // The last legal walk, and the `i53` backing type it forces: a value past
    // `i32` cannot narrow to the default integer.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Walk { A = 9007199254740990, B }
        fun main() {
            let widest: i53 = Walk::B.value();
            print(widest);
        }
        "#,
        "9007199254740991\n",
    );
}

#[test]
fn b111_a_walked_enum_serializes_as_its_backing_value() {
    // §3.9 over the same reading: the derived `Json` shape follows the RUNTIME
    // representation. A walked enum lowered to bare numbers while encoding the
    // variant NAME — `Walked::B` was the number 6 and went out as `"B"`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::{ Json, FromJson };
        import std::result::Result::{ self, Ok, Err };

        [derive(Json)]
        enum Walked { A = 5, B, C }

        fun main() {
            print(Walked::B.to_json());  // 6
            print(match Walked::from_json("6") {
                Ok(let back) => back.value(),
                Err(_) => -1,
            });                          // 6
            print(match Walked::from_json("99") {
                Ok(let back) => back.value(),
                Err(_) => -1,
            });                          // -1
        }
        "#,
        "6\n6\n-1\n",
    );
}

#[test]
fn b111_a_walked_enum_is_hashable_and_keys_by_its_value() {
    // The `Hashable` synthesis always keyed off the runtime representation, so
    // a walked enum already had it. The unification must not narrow that to the
    // generators' stricter rule — this is the pin that would catch it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::map::Map;
        import std::hash::Hashable;

        enum Walked { A = 5, B, C }

        fun main() {
            mut widths: Map<Walked, i32> = Map::new();
            widths.insert(Walked::B, 42);
            print(widths.contains_key(Walked::B));  // true
            print(Walked::B.hash() == 6.hash());    // true — the enum IS 6
        }
        "#,
        "true\ntrue\n",
    );
}

#[test]
fn b111_a_plain_enum_is_still_not_backed() {
    // §3.1(b)'s conjunction is untouched: with no explicit value anywhere the
    // enum keeps its `[index, ..data]` array form and gets no conversions.
    assert_fails_with(
        r#"
        import std::print;
        enum Plain { A, B }
        fun main() { print(Plain::A.value()); }
        "#,
        "Plain has no method 'value'",
    );
}

#[test]
fn b111_a_payload_variant_still_blocks_the_conversions() {
    // §3.3: a payload anywhere flips the enum to the tagged form, so no backing
    // value in it reaches the runtime and no conversion may claim otherwise.
    assert_fails_with(
        r#"
        enum Mixed { A = 1, B(i32) }
        fun main() { }
        "#,
        "an explicit backing value is only meaningful when every variant is \
         data-less, and 'B' carries a payload",
    );
}

// ---------------------------------------------------------------------------
// B115 — a guarded FINAL match leg drops its guard.
//
// The lowering made the final leg the bare `else` on the strength of the
// analyzer's exhaustiveness proof — but that proof counts UNGUARDED legs only
// (a guard tests the value, which the checker does not reason about), so a
// guarded final leg never carried it. Two halves, and the checker's was right:
// it refuses `match a { A => .., B if c => .. }` outright ("missing 'B'"). What
// let the shape through was the checker's *exemptions* — a tuple/generic
// subject skipped the question entirely, and a REFUTABLE tuple pattern counted
// as an irrefutable catch-all. The rule shipped: exhaustiveness is proven by
// unguarded legs only, whatever the subject's type, so a match whose last leg
// is guarded must be exhaustive WITHOUT it; and the lowering keeps that leg's
// test, prelude and guard.
// ---------------------------------------------------------------------------

#[test]
fn b115_a_guarded_final_leg_on_an_enum_is_not_exhaustive() {
    // The checker's half, and it was already right: the coverage walk counts
    // unguarded legs, so 'B' is missing even though the author wrote its name.
    // The note is what makes that legible — it points at the guard.
    assert_fails_noting(
        r#"
        enum E { A, B }
        fun f(e: E, count: i32): str {
            match e {
                E::A => "a",
                E::B if count > 0 => "b",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing 'B'",
        "count > 0",
        "this leg is guarded, and a guarded leg cannot prove exhaustiveness",
    );
}

#[test]
fn b115_a_guarded_final_leg_on_a_backed_enum_is_not_exhaustive() {
    // Same walk, backed subject: the trap arm answers for values outside the
    // SET and never for a guard that rejects an in-set one, so it does not
    // excuse the missing variant.
    assert_fails_with(
        r#"
        enum Align { Start = "s", End = "e" }
        fun f(a: Align, count: i32): str {
            match a {
                Align::Start => "start",
                Align::End if count > 0 => "end",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing 'End'",
    );
}

#[test]
fn b115_a_guarded_final_leg_on_a_tuple_subject_is_refused() {
    // A tuple subject is exempt from the DOMAIN question ("which values can the
    // subject take?"), and that exemption is what B115 rode in on: it never
    // licensed a guard. This program ran the guarded arm when the guard was
    // false.
    assert_fails_noting(
        r#"
        fun f(p: (i32, i32), count: i32): str {
            match p {
                (1, 2) => "x",
                (let a, let b) if count > 0 => "guarded",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: add a catch-all `_` leg",
        "count > 0",
        "this leg is guarded, and a guarded leg cannot prove exhaustiveness",
    );
}

#[test]
fn b115_a_guarded_final_leg_on_a_generic_subject_is_refused() {
    // The same exemption, reached through a generic parameter rather than a
    // tuple — one rule, not two.
    assert_fails_with(
        r#"
        fun f<T>(v: T, count: i32): str {
            match v {
                let x if count > 0 => "guarded",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: add a catch-all `_` leg",
    );
}

#[test]
fn b115_a_lone_guarded_leg_is_refused() {
    // The single-leg shape: nothing precedes the guard, so nothing can carry
    // the proof. It is the smallest program in the family.
    assert_fails_with(
        r#"
        fun f(p: (i32, i32), count: i32): str {
            match p {
                (let a, let b) if count > 0 => "guarded",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: add a catch-all `_` leg",
    );
}

#[test]
fn b115_a_guarded_final_leg_on_an_int_subject_is_refused() {
    // An unbounded scalar domain always needed a catch-all, and a guarded leg
    // is not one — this direction already held, and it is pinned so the shape
    // has a test of its own rather than being inferred from the tuple's.
    assert_fails_with(
        r#"
        fun f(n: i32, count: i32): str {
            match n {
                1 => "one",
                _ if count > 0 => "guarded",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: add a catch-all `_` leg",
    );
}

#[test]
fn b115_a_guarded_final_leg_on_a_str_subject_is_refused() {
    assert_fails_with(
        r#"
        fun f(s: str, count: i32): str {
            match s {
                "quit" => "leaving",
                let other if count > 0 => other,
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: add a catch-all `_` leg",
    );
}

#[test]
fn b115_a_refutable_tuple_pattern_is_not_a_catch_all() {
    // The second half of what let the tuple shape compile: `(1, 2)` is a TEST,
    // and the catch-all walk counted every `ExprPattern::Tuple` as a
    // destructure. With it miscounted, the match above had a "catch-all" and
    // the guarded-final question was never asked.
    assert_fails_with(
        r#"
        fun f(p: (i32, i32), count: i32): str {
            match p {
                (1, 2) => "x",
                (3, let b) if count > 0 => "guarded",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: add a catch-all `_` leg",
    );
}

#[test]
fn b115_a_tuple_of_binders_is_still_a_catch_all() {
    // The other direction of the same walk, so the narrowing cannot silently
    // become "a tuple pattern is never a catch-all": every element is a binder,
    // so the leg matches every value. The guarded leg AFTER it is what makes
    // this non-vacuous — a real catch-all is what licenses one, and the leg is
    // dead where it stands (the lowering stops at the catch-all), so it is the
    // catch-all that is emitted as the `else`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun f(p: (i32, i32), count: i32): str {
            match p {
                (1, 2) => "x",
                (let a, let b) => "rest",
                (3, 4) if count > 0 => "guarded",
            }
        }
        fun main() { print(f((7, 8), 5)); print(f((3, 4), 5)); }
        "#,
        "rest\nrest\n",
    );
}

#[test]
fn b115_a_guarded_leg_before_a_catch_all_runs_both_ways() {
    // The shape the author should write, pinned at RUN time in both
    // directions — the guard is honoured when it holds and the catch-all takes
    // the value when it does not. This is what B115 silently broke when the
    // guarded leg was last instead.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum E { A, B }
        fun f(e: E, count: i32): str {
            match e {
                E::A => "a",
                E::B if count > 0 => "guarded",
                _ => "fallback",
            }
        }
        fun main() {
            print(f(E::B, 5));
            print(f(E::B, 0));
            print(f(E::A, 0));
        }
        "#,
        "guarded\nfallback\na\n",
    );
}

#[test]
fn b115_a_guarded_final_leg_keeps_its_guard() {
    // The lowering's half. The legs before it cover every variant, so the match
    // is exhaustive without the guarded leg and the checker accepts it — and
    // the leg is unreachable, which is exactly why the guard must survive: drop
    // it and the leg becomes the `else` it is not, answering for values the
    // legs above it already took.
    let javascript = compile(
        r#"
        import std::print;
        enum E { A, B }
        fun f(e: E, count: i32): str {
            match e {
                E::A => "a",
                E::B => "b",
                E::B if count > 0 => "guarded",
            }
        }
        fun main() { print(f(E::B, 0)); print(f(E::B, 5)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("} else if ($a[0] === 1 && count > 0) {"),
        "the final leg should keep its test AND its guard, got:\n{javascript}"
    );
    assert!(
        !javascript.contains("} else {\n\t\t$b = \"guarded\";"),
        "the final leg must not become the bare `else`, got:\n{javascript}"
    );
}

#[test]
fn b115_a_guarded_final_leg_runs_the_leg_the_earlier_legs_cover() {
    // The same program, run. It pins the ANSWER, not the guard: an accepted
    // match's guarded final leg is unreachable by construction (the legs that
    // carry the proof take every value first), so dropping the guard is
    // invisible here — measured, by planting the drop and watching this stay
    // green. The emission pins above and below are where the drop shows.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum E { A, B }
        fun f(e: E, count: i32): str {
            match e {
                E::A => "a",
                E::B => "b",
                E::B if count > 0 => "guarded",
            }
        }
        fun main() { print(f(E::B, 0)); print(f(E::B, 5)); print(f(E::A, 5)); }
        "#,
        "b\nb\na\n",
    );
}

#[test]
fn b115_the_trap_composes_with_a_guarded_final_leg() {
    // backed-enums.md §11.3 recorded the collision and left it: "the trap keeps
    // the test and still drops the guard". Both now, and they compose without
    // either learning about the other — the leg keeps test AND guard, and the
    // trap is still the `else`. The message stays honest because an in-set
    // value this guard rejects was taken by one of the legs above.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "s", End = "e" }
        fun f(a: Align, count: i32): str {
            match a {
                Align::Start => "start",
                Align::End => "end",
                Align::End if count > 0 => "guarded",
            }
        }
        fun main() { print(f(Align::End, 0)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("} else if ($a === \"e\" && count > 0) {"),
        "the backed final leg should keep its test AND its guard, got:\n{javascript}"
    );
    assert!(
        javascript.contains("__enum_trap(\"Align\", $a);"),
        "the trap should still be the `else`, got:\n{javascript}"
    );
}

#[test]
fn b115_a_guarded_final_leg_keeps_its_guard_in_the_sequence_emitter() {
    // B59's other emitter (capture-clones.md §5): a guard needing a statement
    // slot — here an `is` test — turns the match into a flat `matched`-flag
    // sequence rather than an else-if chain. The final leg's guard was dropped
    // there too, and with it the prelude the guard's temporary lives in.
    let javascript = compile(
        r#"
        import std::print;
        enum E { A, B }
        enum Wrap { One(i32), Two }
        fun f(e: E, w: Wrap): str {
            match e {
                E::A => "a",
                E::B => "b",
                E::B if w is Wrap::One(let n) && n > 0 => "guarded",
            }
        }
        fun main() { print(f(E::B, Wrap::Two)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("$d = false;"),
        "a guard needing a slot should pick the sequence emitter, got:\n{javascript}"
    );
    assert!(
        javascript.contains("if ($c[0] === 0 && $c[1] > 0) {"),
        "the final leg's guard should survive into its slot, got:\n{javascript}"
    );
}

#[test]
fn b115_a_guarded_final_leg_in_the_sequence_emitter_runs_the_covering_leg() {
    // The same program, run: `w` is `Two`, so the guard rejects and the leg
    // above must be the answer. Like its chain twin, this pins the answer
    // rather than the guard — the guarded leg is unreachable in an accepted
    // match.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum E { A, B }
        enum Wrap { One(i32), Two }
        fun f(e: E, w: Wrap): str {
            match e {
                E::A => "a",
                E::B => "b",
                E::B if w is Wrap::One(let n) && n > 0 => "guarded",
            }
        }
        fun main() { print(f(E::B, Wrap::Two)); print(f(E::B, Wrap::One(3))); }
        "#,
        "b\nb\n",
    );
}

#[test]
fn b115_a_guarded_final_leg_keeps_its_guard_behind_a_nested_proof() {
    // The lowering's half, over a NESTED proof: the coverage that licenses the
    // bare `else` comes from two legs testing below the tag, and the guarded
    // final leg behind them keeps its test, its prelude and its guard exactly
    // as it does behind a flat one.
    //
    // B118 retired this pin's original program. It was written when
    // `Pair::Of(Align::Start)` alone counted as covering the variant `Of`, so
    // the checker accepted a proof that did not hold and the guarded final leg
    // was REACHABLE — the point being that the emission was wrong on its own
    // terms, not merely downstream of a bad verdict. The coverage walk now
    // descends, so that program is refused outright
    // (`b118_a_refutable_payload_pattern_does_not_prove_coverage`) and the
    // shape it demonstrated no longer exists to test.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Pair { Of(Align) }
        fun f(p: Pair, count: i32): str {
            match p {
                Pair::Of(Align::Start) => "s",
                Pair::Of(Align::End) => "e",
                Pair::Of(let x) if count > 0 => "guarded",
            }
        }
        fun main() { print(f(Pair::Of(Align::End), 0)); }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("count > 0"),
        "the final leg's guard should survive, got:\n{javascript}"
    );
    assert!(
        !javascript.contains("} else {\n\t\t$b = \"guarded\";"),
        "the final leg must not become the bare `else`, got:\n{javascript}"
    );
}

#[test]
fn b118_a_refutable_payload_pattern_does_not_prove_coverage() {
    // B115's residual, closed. The match is NOT exhaustive — `Of(End)` with a
    // false guard matches no leg — and the checker now says so. It did not,
    // because the coverage walk asked only which VARIANT a pattern names and
    // never whether the pattern can fail below the tag. The note is what makes
    // the message legible when the leg the author believes covers the case is
    // the guarded one.
    assert_fails_noting(
        r#"
        enum Align { Start, End }
        enum Pair { Of(Align) }
        fun f(p: Pair, count: i32): str {
            match p {
                Pair::Of(Align::Start) => "s",
                Pair::Of(let x) if count > 0 => "guarded",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Pair::Of(Align::End)",
        "count > 0",
        "this leg is guarded, and a guarded leg cannot prove exhaustiveness",
    );
}

// ---------------------------------------------------------------------------
// B118 — the coverage walk's nested and refutable holes (capture-clones.md §12).
//
// One rule, stated over patterns: an unguarded leg proves coverage only for the
// value-space its whole pattern TREE covers, not its root. An enum position is
// covered when every variant is named AND each named variant's payload
// positions are; a tuple position when its elements are; an OPEN position
// (`i32`, `str`, a struct, a still-abstract parameter) only by a binder or `_`.
// What does not change: a guard still proves nothing (B115), and `_` still
// proves everything.
// ---------------------------------------------------------------------------

#[test]
fn b118_a_refutable_payload_pattern_is_not_exhaustive_unguarded() {
    // Shape (a), and the headline: no guard anywhere, one variant, one leg —
    // and before the walk descended this compiled and answered "s" for
    // `Of(End)`. An IN-SET value, silently mislabelled, with no host boundary
    // involved. The witness names the value that had no leg.
    assert_fails_with(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s" }
        }
        fun main() { print(label(Pair::Of(Align::End))); }
        "#,
        "match is not exhaustive: missing Pair::Of(Align::End)",
    );
}

#[test]
fn b118_full_nested_variant_coverage_is_exhaustive() {
    // The other direction, which is what makes the rule a rule and not a ban on
    // nested patterns: name every variant of the payload's enum and the match
    // is total. Run, so the answer is pinned and not merely the verdict.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e" }
        }
        fun main() { print(label(Pair::Of(Align::End))); print(label(Pair::Of(Align::Start))); }
        "#,
        "e\ns\n",
    );
}

#[test]
fn b118_a_binder_or_wildcard_payload_covers_its_variant() {
    // The two irrefutable payload forms, both still total. `Pair::Of(let a)`
    // and `Pair::Of(_)` say nothing about `Align`, which is exactly why they
    // cover all of it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Pair { Of(Align) }
        fun bound(p: Pair): str { match p { Pair::Of(let a) => "bound" } }
        fun wild(p: Pair): str { match p { Pair::Of(_) => "wild" } }
        fun main() { print(bound(Pair::Of(Align::End))); print(wild(Pair::Of(Align::End))); }
        "#,
        "bound\nwild\n",
    );
}

#[test]
fn b118_the_witness_names_the_one_uncovered_variant() {
    // Two of three named: the message must not say "Align" or "the payload",
    // it must name the value that falls through. This is the difference between
    // a diagnostic an author can act on and one they have to decode.
    assert_fails_with(
        r#"
        enum Align { Start, Middle, End }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e" }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Pair::Of(Align::Middle)",
    );
}

#[test]
fn b118_a_tuple_subject_of_literals_is_not_exhaustive() {
    // Shape (b), in its UNGUARDED form — B115 lapsed the tuple exemption only
    // for a guarded final leg, so this compiled and answered "y" for `(7, 8)`.
    // The exemption is gone: a tuple's domain is the product of its elements',
    // which the walk knows how to ask.
    assert_fails_with(
        r#"
        import std::print;
        fun f(p: (i32, i32)): str {
            match p { (1, 2) => "x", (3, 4) => "y" }
        }
        fun main() { print(f((7, 8))); }
        "#,
        "match is not exhaustive: add a catch-all `_` leg",
    );
}

#[test]
fn b118_a_tuple_of_binders_is_still_exhaustive_and_runs() {
    // The tuple direction that must not move: every element a binder, so the
    // leg matches every tuple. `(1, 2)` before it keeps its test.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun f(p: (i32, i32)): str {
            match p { (1, 2) => "x", (let a, let b) => "rest" }
        }
        fun main() { print(f((1, 2))); print(f((7, 8))); }
        "#,
        "x\nrest\n",
    );
}

#[test]
fn b118_a_tuple_of_enums_needs_every_combination() {
    // A tuple whose elements are CLOSED: the domain is finite, so literals are
    // not the problem — a missing combination is. Three of four named, and the
    // witness is the fourth, spelled as the tuple pattern that would cover it.
    assert_fails_with(
        r#"
        enum Align { Start, End }
        fun f(p: (Align, Align)): str {
            match p {
                (Align::Start, Align::Start) => "ss",
                (Align::Start, Align::End) => "se",
                (Align::End, Align::Start) => "es",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing (Align::End, Align::End)",
    );
}

#[test]
fn b118_a_tuple_of_enums_fully_covered_runs() {
    // The same subject with the fourth combination written: total, and the
    // answers are pinned in both elements' directions.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        fun f(p: (Align, Align)): str {
            match p {
                (Align::Start, Align::Start) => "ss",
                (Align::Start, Align::End) => "se",
                (Align::End, Align::Start) => "es",
                (Align::End, Align::End) => "ee",
            }
        }
        fun main() { print(f((Align::End, Align::Start))); print(f((Align::End, Align::End))); }
        "#,
        "es\nee\n",
    );
}

#[test]
fn b118_a_tuple_element_binder_covers_that_element() {
    // Mixed refutability across a tuple's columns: the first element is tested
    // exhaustively, the second is bound. Two legs cover four values.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        fun f(p: (Align, bool)): str {
            match p { (Align::Start, let flag) => "s", (Align::End, let flag) => "e" }
        }
        fun main() { print(f((Align::End, true))); print(f((Align::Start, false))); }
        "#,
        "e\ns\n",
    );
}

#[test]
fn b118_coverage_is_the_union_over_legs_not_a_property_of_one() {
    // The ordering-sensitive shape, and the reason the walk is a matrix rather
    // than a per-leg predicate: neither leg covers the subject, and together
    // they still leave exactly one value — `(End, Start)`. A per-leg rule
    // could only answer "no leg is total", which names nothing.
    assert_fails_with(
        r#"
        enum Align { Start, End }
        fun f(p: (Align, Align)): str {
            match p { (Align::Start, let b) => "s", (let a, Align::End) => "e" }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing (Align::End, Align::Start)",
    );
}

#[test]
fn b118_a_literal_payload_does_not_prove_coverage() {
    // Shape (c): an `i32` payload has no finite set of literals that exhausts
    // it, so no number of legs makes this total. The witness is the binder form
    // that would.
    assert_fails_with(
        r#"
        import std::print;
        enum Wrapped { Of(i32) }
        fun f(w: Wrapped): str {
            match w { Wrapped::Of(1) => "one", Wrapped::Of(2) => "two" }
        }
        fun main() { print(f(Wrapped::Of(9))); }
        "#,
        "match is not exhaustive: missing Wrapped::Of(_)",
    );
}

#[test]
fn b118_a_str_payload_literal_does_not_prove_coverage() {
    // The same argument for the other unbounded scalar — one rule over OPEN
    // domains, not a rule about integers.
    assert_fails_with(
        r#"
        enum Named { Of(str) }
        fun f(n: Named): str {
            match n { Named::Of("a") => "A", Named::Of("b") => "B" }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Named::Of(_)",
    );
}

#[test]
fn b118_a_binder_payload_over_an_open_domain_runs() {
    // And the fix an author writes, run: the literal leg keeps its test and the
    // binder leg takes everything else.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Wrapped { Of(i32) }
        fun f(w: Wrapped): str {
            match w { Wrapped::Of(1) => "one", Wrapped::Of(let n) => "many" }
        }
        fun main() { print(f(Wrapped::Of(1))); print(f(Wrapped::Of(9))); }
        "#,
        "one\nmany\n",
    );
}

#[test]
fn b118_two_levels_of_nesting_are_walked() {
    // Depth, which is where a one-level patch would stop: the hole is two
    // payloads down, and the witness spells the whole path to it.
    assert_fails_with(
        r#"
        enum Align { Start, End }
        enum Mid { Of(Align) }
        enum Outer { Of(Mid) }
        fun f(o: Outer): str {
            match o { Outer::Of(Mid::Of(Align::Start)) => "s" }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Outer::Of(Mid::Of(Align::End))",
    );
}

#[test]
fn b118_a_tuple_inside_a_variant_payload_is_walked() {
    // Mixed nesting, enum-of-tuple: the walk changes shape at each level and
    // the witness reproduces both. The `bool` slot is untested entirely, so it
    // reads `_` rather than an arbitrary one of its two values.
    assert_fails_with(
        r#"
        enum Align { Start, End }
        enum Holder { Of((Align, bool)) }
        fun f(h: Holder): str {
            match h { Holder::Of((Align::Start, let flag)) => "s" }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Holder::Of((Align::End, _))",
    );
}

#[test]
fn b118_a_multi_payload_variant_needs_every_position() {
    // The multi-parameter form: one variant, two payload slots, and coverage is
    // the product. Three of four combinations named, and the witness names the
    // fourth in both slots.
    assert_fails_with(
        r#"
        enum Align { Start, End }
        enum Two { Of(Align, bool) }
        fun f(t: Two): str {
            match t {
                Two::Of(Align::Start, true) => "st",
                Two::Of(Align::Start, false) => "sf",
                Two::Of(Align::End, true) => "et",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Two::Of(Align::End, false)",
    );
}

#[test]
fn b118_a_multi_payload_variant_fully_covered_runs() {
    // Its twin, total and run — the product written out.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Two { Of(Align, bool) }
        fun f(t: Two): str {
            match t {
                Two::Of(Align::Start, true) => "st",
                Two::Of(Align::Start, false) => "sf",
                Two::Of(Align::End, true) => "et",
                Two::Of(Align::End, false) => "ef",
            }
        }
        fun main() { print(f(Two::Of(Align::End, false))); }
        "#,
        "ef\n",
    );
}

#[test]
fn b118_a_recursive_enum_terminates_and_is_walked() {
    // A self-referential payload is the walk's termination question: descending
    // into `Node`'s payload lands back on `Tree`. It terminates because the
    // matrix drives it — a column of binders is dropped, not descended — and
    // the witness is a legal pattern at every depth it reached.
    assert_fails_with(
        r#"
        enum Tree { Leaf, Node(Tree, Tree) }
        fun depth(t: Tree): i32 {
            match t { Tree::Leaf => 0, Tree::Node(Tree::Leaf, let right) => 1 }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Tree::Node(Tree::Node(_, _), _)",
    );
}

#[test]
fn b118_a_recursive_enum_covered_one_level_down_runs() {
    // Its twin: the same shape with the deeper case written is total, and the
    // walk stops there rather than unfolding the type forever.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Tree { Leaf, Node(Tree, Tree) }
        fun depth(t: Tree): i32 {
            match t {
                Tree::Leaf => 0,
                Tree::Node(Tree::Leaf, let right) => 1,
                Tree::Node(Tree::Node(let a, let b), let right) => 2,
            }
        }
        fun main() { print(depth(Tree::Node(Tree::Leaf, Tree::Leaf))); }
        "#,
        "1\n",
    );
}

#[test]
fn b118_a_generic_payload_is_substituted_before_it_is_walked() {
    // `Option<Align>`'s payload is the declared `T` until the matched value's
    // arguments are substituted in — the same substitution `resolve_pattern`
    // applies when it binds `Some(let x)`. Without it the payload column would
    // be an abstract parameter and the hole invisible.
    assert_fails_with(
        r#"
        import std::option::Option;
        enum Align { Start, End }
        fun f(o: Option<Align>): str {
            match o { Option::Some(Align::Start) => "s", Option::None => "n" }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Option::Some(Align::End)",
    );
}

#[test]
fn b118_leg_order_does_not_change_the_verdict() {
    // Coverage is a property of the SET of unguarded legs. The same three legs
    // in a different order are total either way, and the answers follow the
    // legs, not the walk.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Pair { Of(Align), Other }
        fun forward(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e", Pair::Other => "o" }
        }
        fun backward(p: Pair): str {
            match p { Pair::Other => "o", Pair::Of(Align::End) => "e", Pair::Of(Align::Start) => "s" }
        }
        fun main() {
            print(forward(Pair::Of(Align::End)));
            print(backward(Pair::Of(Align::End)));
            print(backward(Pair::Other));
        }
        "#,
        "e\ne\no\n",
    );
}

#[test]
fn b118_an_or_pattern_contributes_each_alternative() {
    // An or-pattern is several patterns sharing a body, and each alternative is
    // its own row in the matrix — so two nested alternatives on ONE leg cover
    // the payload exactly as two legs would.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Pair { Of(Align) }
        fun f(p: Pair): str {
            match p { Pair::Of(Align::Start), Pair::Of(Align::End) => "either" }
        }
        fun main() { print(f(Pair::Of(Align::End))); }
        "#,
        "either\n",
    );
}

#[test]
fn b118_a_guarded_nested_leg_still_proves_nothing() {
    // B115's rule, unchanged one level down: the leg naming `Align::End` is
    // guarded, so the walk does not count it and the payload is holed. The
    // note points at the guard, which is the only reason the author expected
    // the leg to count.
    assert_fails_noting(
        r#"
        enum Align { Start, End }
        enum Pair { Of(Align), Other }
        fun f(p: Pair, count: i32): str {
            match p {
                Pair::Of(Align::Start) => "s",
                Pair::Of(Align::End) if count > 0 => "e",
                Pair::Other => "o",
            }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Pair::Of(Align::End)",
        "count > 0",
        "this leg is guarded, and a guarded leg cannot prove exhaustiveness",
    );
}

#[test]
fn b118_a_catch_all_still_proves_everything_below_the_top_level() {
    // The other thing that does not change: a `_` leg covers every value at
    // every depth, so no amount of refutable nesting before it makes a match
    // non-exhaustive.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, Middle, End }
        enum Mid { Of(Align) }
        enum Outer { Of(Mid) }
        fun f(o: Outer): str {
            match o { Outer::Of(Mid::Of(Align::Start)) => "s", _ => "other" }
        }
        fun main() { print(f(Outer::Of(Mid::Of(Align::End)))); }
        "#,
        "other\n",
    );
}

#[test]
fn b118_a_backed_payload_needs_its_whole_variant_set() {
    // The walk asks the VARIANT SET, and a backed enum's variant set is the
    // same set whatever its values lower to — so shape (a) is refused
    // identically when `Align` is backed. This is the composition point with
    // B114: the trap answers for values outside the set, and it was never the
    // answer to a missing leg inside it.
    assert_fails_with(
        r#"
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s" }
        }
        fun main() { }
        "#,
        "match is not exhaustive: missing Pair::Of(Align::End)",
    );
}

#[test]
fn b118_the_membership_trap_no_longer_fires_on_an_in_set_value() {
    // B114 §12.4's interaction, verified from the other side. Under the hole a
    // match could reach the trap with every value IN set, and the trap then
    // named an in-set value as out-of-set. That match is now refused (pinned
    // above), so the only way to reach the trap is the one it was designed for:
    // a value the HOST invented. Both directions in one program — the in-set
    // payload answers, the invented one traps and names itself.
    let javascript = compile(
        r#"
        import std::print;
        enum Align { Start = "flex-start", End = "flex-end" }
        enum Pair { Of(Align) }
        fun label(p: Pair): str {
            match p { Pair::Of(Align::Start) => "s", Pair::Of(Align::End) => "e" }
        }
        fun main() { print(label(Pair::Of(Align::End))); }
        "#,
    )
    .expect("a clean compile");
    let driven = format!(
        "{javascript}\ntry {{ label([ 0, \"middle\" ]); }} catch (error) {{ console.log(error); }}\n"
    );
    assert_eq!(
        run_js(&driven).expect("a clean run"),
        "e\nAlign: \"middle\" is not one of its values\n",
        "an in-set payload must answer and only an invented one may trap"
    );
}

#[test]
fn b118_the_witness_is_a_pattern_that_closes_the_hole() {
    // The message's contract, checked by using it: whatever the diagnostic
    // names is a legal pattern, and adding it as a leg makes the match total.
    // Pinned across all three witness shapes at once — a nested variant, a
    // multi-slot variant with an untested slot, and a tuple.
    assert_compiles_and_runs(
        r#"
        import std::print;
        enum Align { Start, End }
        enum Holder { Of((Align, bool)) }
        enum Two { Of(Align, bool) }
        fun nested(h: Holder): str {
            match h {
                Holder::Of((Align::Start, let flag)) => "s",
                Holder::Of((Align::End, _)) => "e",
            }
        }
        fun slots(t: Two): str {
            match t {
                Two::Of(Align::Start, true) => "st",
                Two::Of(Align::Start, false) => "sf",
                Two::Of(Align::End, true) => "et",
                Two::Of(Align::End, false) => "ef",
            }
        }
        fun tupled(p: (Align, Align)): str {
            match p {
                (Align::Start, Align::Start) => "ss",
                (Align::Start, Align::End) => "se",
                (Align::End, Align::Start) => "es",
                (Align::End, Align::End) => "ee",
            }
        }
        fun main() {
            print(nested(Holder::Of((Align::End, true))));
            print(slots(Two::Of(Align::End, false)));
            print(tupled((Align::End, Align::End)));
        }
        "#,
        "e\nef\nee\n",
    );
}
