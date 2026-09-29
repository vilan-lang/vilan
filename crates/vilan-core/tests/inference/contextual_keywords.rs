//! B414 — the contextual keywords (`proposal/contextual-keywords.md`).
//!
//! One subject module of the `inference` test binary; the harness it is written
//! against lives in `support.rs`.
//!
//! `with`, `borrows`, `own`, `dyn`, `lazy` and `jump` were reserved words; they
//! are keywords now only where the parser reads them as keywords (lexing.rs
//! `CONTEXTUAL_KEYWORDS`) and ordinary names everywhere else. Each pin holds ONE
//! word in one file at every name position the paper's matrix probed — a
//! binding, a parameter, a closure parameter, a struct field (declared,
//! initialised, read), a method (declared, called), a free function — AND at
//! its keyword reading, and runs it: a demotion that broke the keyword reading
//! would fail the run, and one that left a name position refused would fail
//! the compile. Then the placement rules for a misplaced prefix word, and the
//! `jump` steer.

use crate::support::*;

#[test]
fn b414_with_is_a_name_everywhere_but_an_impl_or_trait_head() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Named { fun name(self): str; }
        trait Greeter with Named { fun greet(self): str; }

        struct Tool { with: i32 }

        impl Tool with Named { fun name(self): str { "tool" } }
        impl Tool with Greeter { fun greet(self): str { i"hi {self.name()}" } }
        impl Tool { fun with(self, with: i32): i32 { self.with + with } }

        fun with(with: i32): i32 { with * 2 }

        fun main() {
            let called = with(1);
            let with = 3;
            let tool = Tool { with = with };
            let apply = |with: i32| with + 1;
            print(i"{tool.with(called)} {tool.with} {apply(with)} {tool.greet()}");
        }
        "#,
        "5 3 4 hi tool\n",
    );
}

#[test]
fn b414_borrows_is_a_name_everywhere_but_after_a_return_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Ledger { borrows: i32 }

        impl Ledger { fun borrows(self): i32 { self.borrows } }

        fun largest(xs: &List<i32>): &i32 borrows xs {
            &xs[1usize]
        }

        fun borrows(borrows: i32): i32 { borrows + 1 }

        fun main() {
            let called = borrows(2);
            let borrows = 2;
            let ledger = Ledger { borrows = borrows };
            let xs = [ 4, 9 ];
            let apply = |borrows: i32| borrows * 10;
            let top: i32 = *largest(&xs);
            print(i"{ledger.borrows()} {called} {apply(borrows)} {top}");
        }
        "#,
        "2 3 20 9\n",
    );
}

#[test]
fn b414_own_is_a_name_everywhere_but_before_a_parameters_binder() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Owner { own: i32 }

        impl Owner { fun own(self): i32 { self.own } }

        fun take(own list: List<i32>): i32 { list.len().as_i32() }
        fun keep(own own: i32): i32 { own }
        fun own(own: i32): i32 { own + 1 }

        fun main() {
            let called = own(5);
            let own = 5;
            let owner = Owner { own = own };
            let apply = |own: i32| own * 2;
            let consume = |own items: List<i32>| items.len().as_i32();
            print(i"{owner.own()} {called} {apply(own)} {take([ 1, 2 ])} {keep(own)} {consume([ 1 ])}");
        }
        "#,
        "5 6 10 2 5 1\n",
    );
}

#[test]
fn b414_dyn_is_a_name_everywhere_but_a_type_head() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Shape { fun area(self): i32; }

        struct Square { dyn: i32 }

        impl Square with Shape { fun area(self): i32 { self.dyn * self.dyn } }
        impl Square { fun dyn(self): i32 { self.dyn } }

        fun measure(shape: dyn Shape): i32 { shape.area() }
        fun dyn(dyn: i32): i32 { dyn - 1 }

        fun main() {
            let called = dyn(3);
            let dyn = 3;
            let square = Square { dyn = dyn };
            let apply = |dyn: i32| dyn + 100;
            let shapes: List<dyn Shape> = [ square ];
            print(i"{square.dyn()} {called} {apply(dyn)} {measure(square)} {shapes.len()}");
        }
        "#,
        "3 2 103 9 1\n",
    );
}

#[test]
fn b414_lazy_is_a_name_everywhere_but_before_a_let_or_a_parameters_binder() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Pace { lazy: i32 }

        impl Pace { fun lazy(self): i32 { self.lazy } }

        lazy let config: i32 = 40;

        fun or_else(value: i32, lazy fallback: i32): i32 {
            if value > 0 { value } else { fallback }
        }
        fun lazy(lazy: i32): i32 { lazy + config }

        fun main() {
            let called = lazy(2);
            let lazy = 2;
            let pace = Pace { lazy = lazy };
            let apply = |lazy: i32| lazy * 3;
            mut count = lazy;
            count = count + lazy;
            print(i"{pace.lazy()} {called} {apply(lazy)} {or_else(0, 7)} {count}");
        }
        "#,
        "2 42 6 7 4\n",
    );
}

#[test]
fn b414_jump_is_a_name_everywhere_but_before_its_target() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Player { jump: i32 }

        impl Player { fun jump(self): i32 { self.jump } }

        fun jump(jump: i32): i32 { jump + 1 }

        fun main() {
            let called = jump(2);
            let jump = 2;
            let player = Player { jump = jump };
            let apply = |jump: i32| jump * 5;
            mut total = 0;
            for x in [ 1, 2, 3, 4 ] {
                if x == jump { jump continue; }
                if x > player.jump() + 1 { jump break; }
                total += x;
            }
            print(i"{player.jump()} {called} {apply(jump)} {total}");
        }
        "#,
        "2 3 10 4\n",
    );
}

// --- The placement rules (contextual-keywords.md §8) ------------------------

#[test]
fn b414_own_outside_a_parameter_head_names_its_placement() {
    assert_fails_with(
        "fun main() {\n\tlet own x = 1;\n}\n",
        "`own` is a PARAMETER convention",
    );
    assert_fails_with(
        "fun f(x: own i32): i32 { x }\n",
        "`own` is a PARAMETER convention",
    );
}

#[test]
fn b414_dyn_in_an_expression_names_its_placement() {
    assert_fails_with(
        "trait Show { fun show(self): str; }\nfun main() {\n\tlet x = dyn Show;\n}\n",
        "`dyn` marks a trait object in TYPE position",
    );
}

#[test]
fn b414_a_type_head_dyn_without_a_trait_still_asks_for_one() {
    assert_fails_with(
        "fun main() {\n\tlet x: dyn = 1;\n}\n",
        "a trait name after `dyn`",
    );
}

#[test]
fn b414_a_forgotten_jump_target_is_steered_to_the_two_targets() {
    assert_fails_with(
        "fun main() {\n\tfor x in [ 1 ] {\n\t\tjump;\n\t}\n}\n",
        "`jump` is loop control and names its target: `jump break;` or `jump continue;`",
    );
}

#[test]
fn b414_the_missing_binder_word_after_lazy_is_still_reported() {
    assert_fails_with(
        "lazy config = 1;\nfun main() {}\n",
        "a lazy binding is `lazy let name: T = <initializer>;`",
    );
}

// --- S4: the member tier (contextual-keywords.md §5, Q2; R-k RULED 2026-09-29) --
//
// Every word — the RESERVED ones included — names a member: a field declared in
// a struct body and given with `=` in a literal, a field read and a method
// called after `.`/`?.`, a method declared in an `impl` or `trait`, a static
// reached after `::`. Each position is entered after a token that commits to
// it, so no production can start there with the keyword. R-k is the price of
// admitting the word after `.`: the name is written AGAINST the dot, so a
// half-typed `value.` never reads the next line's `let` as its member.

/// Every reserved word, at every member position, in one program that runs —
/// the fields sum to 378 (0..=27), the methods add 100 each.
#[test]
fn b414_s4_every_reserved_word_names_a_member_at_every_member_position() {
    let words = [
        "async", "await", "const", "css", "else", "enum", "export", "external", "false", "for",
        "fun", "if", "impl", "import", "in", "is", "let", "macro", "match", "mod", "mut", "null",
        "ret", "struct", "trait", "true", "type", "use",
    ];
    assert_eq!(
        words.len(),
        vilan_core::lexing::KEYWORDS.len(),
        "every reserved word is exercised (lexing::KEYWORDS grew or shrank)"
    );
    let mut source = String::from("import std::io::print;\n\nstruct Bag {\n");
    for word in words {
        source.push_str(&format!("    {word}: i32,\n"));
    }
    source.push_str("}\n\nimpl Bag {\n");
    for word in words {
        source.push_str(&format!(
            "    fun {word}(self): i32 {{ self.{word} + 100 }}\n"
        ));
    }
    source.push_str("    fun make(): Bag {\n        Bag { ");
    let initializers: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(index, word)| format!("{word} = {index}"))
        .collect();
    source.push_str(&initializers.join(", "));
    source.push_str(" }\n    }\n}\n\nstruct Kit {}\n\nimpl Kit {\n");
    for word in words {
        source.push_str(&format!("    fun {word}(): str {{ \"{word}\" }}\n"));
    }
    source.push_str("}\n\nfun main() {\n    let bag = Bag::make();\n    mut total = 0;\n");
    for word in words {
        source.push_str(&format!("    total = total + bag.{word} + bag.{word}();\n"));
    }
    source.push_str("    print(total);\n    mut names = \"\";\n");
    for word in words {
        source.push_str(&format!("    names = names + Kit::{word}();\n"));
    }
    source.push_str("    print(names);\n}\nmain();\n");
    assert_compiles_and_runs(&source, &format!("3556\n{}\n", words.concat()));
}

/// The program the member tier was asked for (paper §5 exhibit 1): a JSON
/// `"type"` key, declared, initialised, read, encoded AND decoded by the
/// derive — `[derive(Json)]` spells a key as a field, and there was no
/// field to spell this one with.
#[test]
fn b414_s4_a_json_type_key_round_trips_through_the_derive() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };

        [derive(Json)]
        struct Event {
            type: str,
            id: i32,
        }

        fun main() {
            let event = Event { type = "click", id = 1 };
            print(event.to_json());
            match Event::from_json("{\"type\":\"key\",\"id\":2}") {
                Ok(let decoded) => print(i"{decoded.type} {decoded.id}"),
                Err(let message) => print(i"refused: {message}"),
            }
        }
        main();
        "#,
        "{\"type\":\"click\",\"id\":1}\nkey 2\n",
    );
}

/// A lifted link, a trait's required and default methods, and a method call
/// with generic arguments — the member positions the big program does not
/// reach.
#[test]
fn b414_s4_reserved_members_through_a_lift_a_trait_and_generics() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Shape {
            fun type(self): str;
            fun match(self): str { i"a {self.type()}" }
        }

        struct Square { side: i32 }

        impl Square with Shape {
            fun type(self): str { "square" }
        }

        impl Square {
            fun in<T>(self, value: T): T { value }
        }

        fun main() {
            let found: Option<Square> = Some(Square { side = 2 });
            let kind: Option<str> = found?.type();
            match kind {
                Some(let name) => print(name),
                None => print("none"),
            }
            print(Square { side = 1 }.match());
            print(Square { side = 1 }.in<i32>(7));
        }
        main();
        "#,
        "square\na square\n7\n",
    );
}

/// The member tier is MEMBER positions only: a reserved word still cannot
/// name a binding, a parameter, a free function, or a field written as the
/// shorthand `{ type }` (which reads a binding of that name).
#[test]
fn b414_s4_a_reserved_word_still_names_no_binding_parameter_or_free_function() {
    assert_fails("fun main() {\n\tlet type = 1;\n}\n");
    assert_fails("fun f(type: i32): i32 { 1 }\nfun main() {}\n");
    assert_fails("fun type(): i32 { 1 }\nfun main() {}\n");
    assert_fails(
        "struct Event { type: str }\nfun main() {\n\tlet t = \"x\";\n\tlet e = Event { type };\n}\n",
    );
}

/// R-k, same line: a space between the dot and its name is refused with the
/// steer — and the member is still READ, so the rest of the statement
/// analyzes (one diagnostic, no cascade).
#[test]
fn b414_s4_a_space_after_a_member_dot_is_refused_with_the_steer() {
    assert_fails_once_with(
        "struct S { n: i32 }\nfun main() {\n\tlet s = S { n = 1 };\n\tlet m = s. n;\n\tlet k = m + 1;\n}\n",
        "found 'n' expected a member name written against its `.`",
    );
    assert_fails_with(
        "fun main() {\n\tlet xs = [ 1 ];\n\tlet n = xs. len();\n}\n",
        "a member name written against its `.`",
    );
    // A tuple index and a lifted link are members too.
    assert_fails_with(
        "fun main() {\n\tlet t = (1, 2);\n\tlet a = t. 0;\n}\n",
        "a member name written against its `.`",
    );
    assert_fails_with(
        "fun main() {\n\tlet o: Option<(i32, i32)> = None;\n\tlet a = o?. 0;\n}\n",
        "a member name written against its `.`",
    );
}

/// R-k, across a line: a `.` that ends its line never takes the next line's
/// first word as its member. A stranded name is refused where it stands; a
/// half-typed `value.` above a STATEMENT keeps the mid-edit recovery it always
/// had — the statement is kept, so the next line's `let b` still binds and
/// nothing downstream cascades — and even under the member tier the `let` is
/// never read as the member.
#[test]
fn b414_s4_a_member_dot_at_the_end_of_a_line_takes_nothing_from_the_next() {
    assert_fails_with(
        "struct S { n: i32 }\nfun main() {\n\tlet s = S { n = 1 };\n\tlet a = s.\n\t\tn;\n}\n",
        "found 'n' expected a member name written against its `.`",
    );
    let half_typed = "struct S { n: i32 }\nfun main() {\n\tlet s = S { n = 1 };\n\tlet a = s.\n\tlet b = 2;\n\tlet c = b + 1;\n}\n";
    assert_fails_with(half_typed, "expected a field or method name after `.`");
    assert_fails_without(half_typed, "cannot find 'b'");
}

/// The multi-line chain breaks BEFORE the dot, and R-k leaves it alone.
#[test]
fn b414_s4_a_chain_broken_before_its_dots_is_untouched() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let xs = [ 3, 1, 2 ];
            let n = xs
                .len();
            print(n);
        }
        main();
        "#,
        "3\n",
    );
}
