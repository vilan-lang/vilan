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
