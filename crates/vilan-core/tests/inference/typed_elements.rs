//! Typed elements (native-ui.md §12): a tag whose name resolves, in the scope
//! of the element, to a function declared `[element]` lowers to a method chain
//! on what that function returns — `<row gap(dp(8)) wrap>a{b}</row>` is
//! `row().gap(dp(8)).wrap().child("a").child(b)` — and is checked as that
//! chain. Every other tag keeps the web lowering (`view("tag")` and its
//! string-keyed links), which the corpus goldens hold byte for byte.
//!
//! Each fixture declares its own small builder: std declares no `[element]`
//! function yet.

use super::support::*;

/// A builder that records every call it receives, in order, so a run shows the
/// chain the element lowered to.
const ROW: &str = r#"
struct Row {
    log: str,
}

impl Row {
    fun gap(self, value: i32): Row {
        Row { log = self.log + i" gap={value}" }
    }
    fun wrap(self): Row {
        Row { log = self.log + " wrap" }
    }
    fun child(self, value: str): Row {
        Row { log = self.log + " child=" + value }
    }
}

[element]
fun row(): Row {
    Row { log = "row" }
}
"#;

fn with_row(main: &str) -> String {
    format!("{ROW}\n{main}")
}

// --- The lowering ------------------------------------------------------------

#[test]
fn a_typed_element_lowers_to_its_builders_method_chain() {
    // The table's every row at once: the constructor call, an attribute with
    // a value, a bare attribute, a literal text child and an expression child.
    assert_compiles_and_runs(
        &with_row(
            r#"
            fun main() {
                let b = "bee";
                print(<row gap(8) wrap>"a"{b}</row>.log);
            }
            "#,
        ),
        "row gap=8 wrap child=a child=bee\n",
    );
}

#[test]
fn a_typed_elements_attributes_are_called_in_source_order() {
    // No reordering: `wrap` before `gap` is called first.
    assert_compiles_and_runs(
        &with_row(
            r#"
            fun main() {
                print(<row wrap gap(1) wrap />.log);
                print(<row gap(1) wrap gap(2) />.log);
            }
            "#,
        ),
        "row wrap gap=1 wrap\nrow gap=1 wrap gap=2\n",
    );
}

#[test]
fn a_self_closing_typed_element_is_the_bare_constructor_call() {
    assert_compiles_and_runs(
        &with_row(
            r#"
            fun main() {
                print(<row />.log);
                print(<row></row>.log);
            }
            "#,
        ),
        "row\nrow\n",
    );
}

#[test]
fn a_conditional_hole_is_one_child() {
    assert_compiles_and_runs(
        &with_row(
            r#"
            fun main() {
                let flag = false;
                print(<row>{flag then "y" else "n"}{if flag { "a" } else { "b" }}</row>.log);
            }
            "#,
        ),
        "row child=n child=b\n",
    );
}

#[test]
fn a_typed_element_nests_in_a_typed_element() {
    // The inner elements are the outer one's children, each lowered first.
    assert_compiles_and_runs(
        &with_row(
            r#"
            struct Column {
                log: str,
            }

            impl Column {
                fun child(self, value: Row): Column {
                    Column { log = self.log + " [" + value.log + "]" }
                }
            }

            [element]
            fun column(): Column {
                Column { log = "column" }
            }

            fun main() {
                print(<column><row gap(1) />{<row wrap>"x"</row>}</column>.log);
            }
            "#,
        ),
        "column [row gap=1] [row wrap child=x]\n",
    );
}

#[test]
fn a_web_element_nests_in_a_typed_element_and_a_typed_one_in_a_web_one() {
    // A builder may be the web `View` itself: `para()` answers one, so
    // `<para text("hi")>` is `para().text("hi")` — `text` is `View`'s own
    // method, not an attribute — while the `<div>` around it and the `<b>`
    // inside it stay web elements.
    assert_compiles_and_runs(
        r#"
        import std::web::ui::{ view, View, render };

        [element]
        fun para(): View {
            view("p")
        }

        fun main() {
            print(render(<div class("x")><para text("hi")><b>"!"</b></para>"t"</div>));
        }
        "#,
        "<div class=\"x\"><p>hi<b>!</b></p>t</div>\n",
    );
}

#[test]
fn a_fragment_child_of_a_typed_element_is_one_list_argument() {
    // A46's fragment is a list literal wherever it stands, so the builder's
    // `child` receives the list whole.
    assert_compiles_and_runs(
        r#"
        struct Group {
            count: usize,
        }

        impl Group {
            fun child(self, values: List<str>): Group {
                Group { count = self.count + values.len() }
            }
        }

        [element]
        fun group(): Group {
            Group { count = 0 }
        }

        fun main() {
            print(<group><>"a" "b" "c"</></group>.count);
        }
        "#,
        "3\n",
    );
}

/// A generic builder, shared by the two pins below.
const STACK: &str = r#"
struct Stack<T> {
    items: List<T>,
}

impl Stack<type T> {
    fun child(self, value: T): Stack<T> {
        mut items = self.items;
        items.push(value);
        Stack { items }
    }
}
"#;

#[test]
fn a_generic_builder_is_instantiated_by_its_constructor() {
    assert_compiles_and_runs(
        &format!(
            "{STACK}
{}",
            r#"
            [element]
            fun ints(): Stack<i32> {
                Stack { items = List::new() }
            }

            fun main() {
                let built = <ints>{1}{2}{3}</ints>;
                print(built.items.len());
            }
            "#
        ),
        "3
",
    );
}

#[test]
fn a_generic_element_function_is_inferred_as_its_written_chain_is() {
    // The tag has no type-argument form. What binds the constructor's `T` is
    // whatever binds it in the chain the element lowers to, so the two
    // spellings agree, case by case — including where neither binds it.
    let program = |binding: &str| {
        format!(
            "{STACK}\n[element]\nfun stack<T>(): Stack<T> {{\n    Stack {{ items = List::new() }}\n}}\n\nfun main() {{\n    {binding};\n    print(built.items.len());\n}}\n"
        )
    };
    let cases = [
        (
            "let built: Stack<i32> = <stack>{1}</stack>",
            "let built: Stack<i32> = stack().child(1)",
        ),
        (
            "let built: Stack<i32> = <stack>{1}{2}{3}</stack>",
            "let built: Stack<i32> = stack().child(1).child(2).child(3)",
        ),
        (
            "let built = <stack>{1}</stack>",
            "let built = stack().child(1)",
        ),
    ];
    let mut refused = 0;
    for (element, chain) in cases {
        let element_outcome = compile(&program(element)).map(|_| ());
        let chain_outcome = compile(&program(chain)).map(|_| ());
        assert_eq!(element_outcome, chain_outcome, "{element} against {chain}");
        refused += usize::from(element_outcome.is_err());
    }
    // Not vacuous: the set holds both outcomes.
    assert!(refused > 0 && refused < cases.len());
}

#[test]
fn a_builder_method_may_consume_or_borrow_its_receiver() {
    // `own self` takes the builder and hands the same one back; a bare `self`
    // reads it and answers a new one. Both chain.
    assert_compiles_and_runs(
        r#"
        struct Row {
            log: str,
        }

        impl Row {
            fun gap(own self, value: i32): Row {
                self.log = self.log + i" gap={value}";
                self
            }
            fun wrap(self): Row {
                Row { log = self.log + " wrap" }
            }
        }

        [element]
        fun row(): Row {
            Row { log = "row" }
        }

        fun main() {
            print(<row gap(1) wrap gap(2) />.log);
        }
        "#,
        "row gap=1 wrap gap=2\n",
    );
}

#[test]
fn a_typed_element_evaluates_at_compile_time_under_const() {
    // The native evaluator runs the same lowered chain.
    assert_compiles_and_runs(
        &with_row(
            r#"
            const let built = <row gap(3)>"k"</row>;

            fun main() {
                print(built.log);
            }
            "#,
        ),
        "row gap=3 child=k\n",
    );
}

#[test]
fn a_typed_element_is_a_receiver_like_any_value() {
    // A link written AFTER the element is an ordinary call on the builder.
    assert_compiles_and_runs(
        &with_row(
            r#"
            fun main() {
                print(<row gap(1) />.wrap().log);
            }
            "#,
        ),
        "row gap=1 wrap\n",
    );
}

#[test]
fn the_emitted_chain_calls_the_constructor_and_no_view() {
    let javascript = compile(&with_row(
        r#"
        fun main() {
            print(<row gap(1)>"a"</row>.log);
        }
        "#,
    ))
    .expect("compiles");
    assert!(javascript.contains("row()"), "{javascript}");
    assert!(!javascript.contains("view("), "{javascript}");
    assert!(!javascript.contains("\"gap\""), "{javascript}");
}

// --- Resolution --------------------------------------------------------------

#[test]
fn a_function_named_like_a_tag_without_the_marker_keeps_the_web_lowering() {
    // Web programs declare `button`, `text`, `label`: without `[element]` the
    // tag is the web element it always was.
    let javascript = compile(
        r#"
        import std::web::ui::{ view, View, render };

        fun div(): i32 {
            1
        }

        fun main() {
            print(render(<div class("x") />));
            print(div());
        }
        "#,
    )
    .expect("compiles");
    assert!(javascript.contains("view(\"div\")"), "{javascript}");
    assert!(javascript.contains("\"class\""), "{javascript}");
}

#[test]
fn a_local_binding_shadows_an_element_function() {
    // The tag resolves where it is written: the `let` hides the `[element]`
    // function, so `<row>` is a web element and `gap` a string-keyed
    // attribute.
    assert_compiles_and_runs(
        &with_row(
            r#"
            import std::web::ui::{ view, View, render };

            fun main() {
                let row = 3;
                print(render(<row gap("1") />));
                print(row);
            }
            "#,
        ),
        "<row gap=\"1\"></row>\n3\n",
    );
}

#[test]
fn a_binding_declared_after_the_element_does_not_shadow_it() {
    // Positional visibility: a `let` later in the block is not in scope at
    // the tag.
    assert_compiles_and_runs(
        &with_row(
            r#"
            fun main() {
                print(<row wrap />.log);
                let row = 3;
                print(row);
            }
            "#,
        ),
        "row wrap\n3\n",
    );
}

#[test]
fn a_local_element_function_makes_its_tag_typed() {
    assert_compiles_and_runs(
        r#"
        struct Tally {
            count: i32,
        }

        impl Tally {
            fun child(self, value: i32): Tally {
                Tally { count = self.count + value }
            }
        }

        fun main() {
            [element]
            fun tally(): Tally {
                Tally { count = 0 }
            }
            print(<tally>{2}{3}</tally>.count);
        }
        "#,
        "5\n",
    );
}

#[test]
fn a_local_function_without_the_marker_shadows_an_element_function() {
    assert_compiles_and_runs(
        &with_row(
            r#"
            import std::web::ui::{ view, View, render };

            fun main() {
                fun row(): i32 {
                    7
                }
                print(render(<row wrap />));
                print(row());
            }
            "#,
        ),
        "<row wrap=\"\"></row>\n7\n",
    );
}

// --- Through imports ---------------------------------------------------------

const UI_MODULE: &str = r#"
export struct Row {
    log: str,
}

export impl Row {
    fun gap(self, value: i32): Row {
        Row { log = self.log + i" gap={value}" }
    }
    fun child(self, value: str): Row {
        Row { log = self.log + " child=" + value }
    }
}

[element]
export fun row(): Row {
    Row { log = "row" }
}
"#;

fn run_with_ui_module(main: &str) -> String {
    let javascript = crate::modules::compile_package(
        &[
            ("vilan.toml", "[package]\nname = \"typed\"\nroot = \".\"\n"),
            ("ui.vl", UI_MODULE),
            ("main.vl", main),
        ],
        "main.vl",
    )
    .unwrap_or_else(|errors| panic!("expected a clean compile, got: {errors:#?}"));
    run_js(&javascript).unwrap_or_else(|errors| panic!("expected a clean run, got: {errors:#?}"))
}

#[test]
fn an_imported_element_function_makes_its_tag_typed() {
    assert_eq!(
        run_with_ui_module(
            "import pkg::ui::row;\n\nfun main() {\n\tprint(<row gap(1)>\"a\"</row>.log);\n}\n"
        ),
        "row gap=1 child=a\n"
    );
}

#[test]
fn a_renamed_import_makes_the_new_name_the_tag() {
    assert_eq!(
        run_with_ui_module(
            "import pkg::ui::{ row as line };\n\nfun main() {\n\tprint(<line gap(2)>\"b\"</line>.log);\n}\n"
        ),
        "row gap=2 child=b\n"
    );
}

#[test]
fn an_element_function_another_module_declares_but_this_one_does_not_import_stays_web() {
    // `ui::row` exists but is not in scope here: `<row>` is std's.
    let javascript = crate::modules::compile_package(
        &[
            ("vilan.toml", "[package]\nname = \"typed\"\nroot = \".\"\n"),
            ("ui.vl", UI_MODULE),
            (
                "main.vl",
                "import std::web::ui::{ view, View, render };\nimport pkg::ui;\n\nfun main() {\n\tprint(render(<row gap(\"1\") />));\n}\n",
            ),
        ],
        "main.vl",
    )
    .unwrap_or_else(|errors| panic!("expected a clean compile, got: {errors:#?}"));
    assert_eq!(
        run_js(&javascript).expect("runs"),
        "<row gap=\"1\"></row>\n"
    );
}

// --- Diagnostics -------------------------------------------------------------

#[test]
fn an_unknown_attribute_is_an_unknown_method_at_the_attribute() {
    assert_fails_spanning(
        &with_row("fun main() {\n\tlet built = <row padding(3) />;\n}\n"),
        "padding",
        "Row has no method 'padding'",
    );
}

#[test]
fn a_bare_unknown_attribute_is_an_unknown_method_at_its_name() {
    assert_fails_spanning(
        &with_row("fun main() {\n\tlet built = <row hidden />;\n}\n"),
        "hidden",
        "Row has no method 'hidden'",
    );
}

#[test]
fn an_attribute_value_of_the_wrong_type_is_the_methods_mismatch_at_the_value() {
    assert_fails_spanning(
        &with_row("fun main() {\n\tlet built = <row gap(\"wide\") />;\n}\n"),
        "\"wide\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn a_bare_attribute_on_a_method_that_takes_a_value_is_an_arity_error() {
    assert_fails_with(
        &with_row("fun main() {\n\tlet built = <row gap />;\n}\n"),
        "`gap` expects 1 argument",
    );
}

#[test]
fn an_attribute_value_on_a_method_that_takes_none_is_an_arity_error() {
    assert_fails_with(
        &with_row("fun main() {\n\tlet built = <row wrap(1) />;\n}\n"),
        "`wrap` expects 0 arguments",
    );
}

#[test]
fn an_event_item_on_a_typed_element_is_refused_by_name() {
    assert_fails_spanning(
        &with_row("fun main() {\n\tlet built = <row on:click(|| print(\"x\")) />;\n}\n"),
        "click(|| print(\"x\")",
        "`on:` is the web element's event form: on `<row>`",
    );
    // One diagnostic: the refused link stays quiet.
    let diagnostics = failure_diagnostics(&with_row(
        "fun main() {\n\tlet built = <row on:click(|| print(\"x\")) />;\n}\n",
    ));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
}

#[test]
fn a_dotted_link_on_a_typed_element_is_refused_by_name() {
    assert_fails_spanning(
        &with_row("fun main() {\n\tlet built = <row .wrap() />;\n}\n"),
        "wrap()",
        "a `.method(…)` link is the web element's head form: on `<row>`",
    );
    // One diagnostic, even for a link no method answers: the refused link
    // stays quiet.
    let diagnostics = failure_diagnostics(&with_row(
        "fun main() {\n\tlet built = <row .nothing() gap(1) />;\n}\n",
    ));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
}

#[test]
fn a_dotted_field_link_on_a_typed_element_is_refused_by_name() {
    assert_fails_spanning(
        "struct Leaf {\n\ttint: i32,\n}\n\n[element]\nfun leaf(): Leaf {\n\tLeaf { tint = 1 }\n}\n\nfun main() {\n\tlet built = <leaf .shade />;\n}\n",
        "shade",
        "a `.method(…)` link is the web element's head form: on `<leaf>`",
    );
}

#[test]
fn a_hyphenated_attribute_on_a_typed_element_is_refused_by_name() {
    assert_fails_spanning(
        &with_row("fun main() {\n\tlet built = <row aria-label(\"x\") />;\n}\n"),
        "aria-label",
        "`aria-label` is a hyphenated attribute, which only a web element takes",
    );
    let diagnostics = failure_diagnostics(&with_row(
        "fun main() {\n\tlet built = <row aria-label(\"x\") />;\n}\n",
    ));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
}

#[test]
fn a_child_the_builder_does_not_take_is_the_child_methods_mismatch() {
    assert_fails_with(
        &with_row("fun main() {\n\tlet built = <row>{1}</row>;\n}\n"),
        "Expected str, but got i32 instead.",
    );
}

#[test]
fn a_builder_without_a_child_method_refuses_children() {
    assert_fails_with(
        r#"
        struct Leaf {}

        [element]
        fun leaf(): Leaf {
            Leaf {}
        }

        fun main() {
            let built = <leaf>"x"</leaf>;
        }
        "#,
        "Leaf has no method 'child'",
    );
}

// --- The attribute -----------------------------------------------------------

#[test]
fn the_element_attribute_refuses_parameters() {
    assert_fails_spanning(
        "[element]\nfun row(gap: i32): i32 {\n\tgap\n}\n",
        "(gap: i32)",
        "an `[element]` function is called by its tag with no arguments",
    );
}

#[test]
fn the_element_attribute_refuses_a_method() {
    assert_fails_spanning(
        "struct T {}\n\nimpl T {\n\t[element]\n\tfun make(): T {\n\t\tT {}\n\t}\n}\n",
        "[element]",
        "a method is reached through a receiver",
    );
}

#[test]
fn the_element_attribute_refuses_an_external_function() {
    assert_fails_spanning(
        "[extern(\"make\")]\n[element]\nexternal fun make(): i32;\n",
        "[element]",
        "an `external fun` binds a host function",
    );
}

#[test]
fn the_element_attribute_refuses_a_declaration_that_is_not_a_function() {
    for source in [
        "[element]\nstruct Row {}\n",
        "[element]\nenum Kind {\n\tA,\n}\n",
        "fun main() {\n\t[element]\n\tlet x = 1;\n}\n",
    ] {
        assert_fails_spanning(
            source,
            "[element]",
            "`[element]` marks the function a tag calls by name: it may label only a free `fun`",
        );
    }
}

#[test]
fn the_element_attribute_takes_its_place_in_the_canonical_order() {
    // The class band, after `[platform]`.
    assert_compiles(&with_row(
        "[deprecated(\"use row\")]\n[element]\nfun line(): Row {\n\tRow { log = \"line\" }\n}\n\nfun main() {}\n",
    ));
    assert_fails_with(
        "[element]\n[must_use]\nfun line(): i32 {\n\t1\n}\n",
        "a declaration's attributes are written in one order",
    );
}
