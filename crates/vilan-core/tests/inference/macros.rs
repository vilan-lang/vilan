//! The macro engine, block-scoped imports, sized numerics, the ambient owner,
//! and the dependency-package demotion contract (E84/E90).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- The bare-`?` TRAIT path (expression-lifting.md §4, B11's tail) ---
//
// A user `Lift` container in a region lowers to nested `and_then` calls
// ending in `map` — its OWN members, not the inline std form. Every pin below
// reads the tag each member appends, so a passing run proves *which* member
// ran and in what order; a value-only assertion could not tell the trait path
// from the std one.

/// The shared fixture: `Boxy` tags every member it runs through.
const USER_LIFT_CONTAINER: &str = r#"
        import std::io::print;
        import std::display::format;
        import std::operators::Lift;

        struct Boxy<T> { value: T, tag: str }

        impl Boxy<type T> with Lift {}

        impl Boxy<type T> {
            fun map<U>(self, fn: |T| U): Boxy<U> {
                Boxy { value = fn(self.value), tag = self.tag + ".map" }
            }

            fun and_then<U>(self, fn: |T| Boxy<U>): Boxy<U> {
                let inner = fn(self.value);
                Boxy { value = inner.value, tag = self.tag + "+" + inner.tag }
            }
        }
"#;

#[test]
fn expression_lift_on_a_user_container_maps_through_its_own_map() {
    // One receiver, a plain body: the region is `boxed.map(|x| x * 2)`, and
    // the `.map` the tag picks up is the user's method — not the inline
    // tag-branch lowering the std pair gets.
    assert_compiles_and_runs(
        &format!(
            "{USER_LIFT_CONTAINER}
        fun main() {{
            let boxed = Boxy {{ value = 20, tag = \"a\" }};
            let doubled: Boxy<i32> = boxed? * 2;
            print(i\"{{format(doubled.value)}} [{{doubled.tag}}]\");
        }}
        "
        ),
        "40 [a.map]\n",
    );
}

#[test]
fn expression_lift_on_a_user_container_nests_and_then_ending_in_map() {
    // Two receivers: `left.and_then(|x| right.map(|y| x + y))` — §4's
    // "nested `and_then` calls ending in `map`". The tag pins the whole
    // shape: `R.map` is the inner member, `L+…` the outer `and_then`'s
    // concatenation, so a flat or reversed nesting would read differently.
    assert_compiles_and_runs(
        &format!(
            "{USER_LIFT_CONTAINER}
        fun main() {{
            let left = Boxy {{ value = 40, tag = \"L\" }};
            let right = Boxy {{ value = 2, tag = \"R\" }};
            let total: Boxy<i32> = left? + right?;
            print(i\"{{format(total.value)}} [{{total.tag}}]\");
        }}
        "
        ),
        "42 [L+R.map]\n",
    );
}

#[test]
fn expression_lift_on_a_user_container_flattens_through_and_then() {
    // The body yields the receiver's own container (`rows?[0]` on a
    // `Boxy<List<Boxy<i32>>>`), so the region is ONE level — `and_then`, not
    // `map` (the chain rule inherited). The annotation pins the type; the
    // tag's `+` (and_then's join, never map's `.map`) pins the member.
    assert_compiles_and_runs(
        &format!(
            "{USER_LIFT_CONTAINER}
        fun main() {{
            let rows: Boxy<List<Boxy<i32>>> = Boxy {{
                value = [Boxy {{ value = 7, tag = \"inner\" }}],
                tag = \"outer\",
            }};
            let first: Boxy<i32> = rows?[0];
            print(i\"{{format(first.value)}} [{{first.tag}}]\");
        }}
        "
        ),
        "7 [outer+inner]\n",
    );
}

#[test]
fn expression_lift_on_a_user_container_orders_effects_left_to_right() {
    // §4: "Left-to-right, so effects order as written." The right receiver
    // is built inside the left's continuation, and a hoisted eval step
    // between them runs there too — L, M, R, in source order.
    assert_compiles_and_runs(
        &format!(
            "{USER_LIFT_CONTAINER}
        fun boxed(label: str, value: i32): Boxy<i32> {{
            print(label);
            Boxy {{ value = value, tag = label }}
        }}

        fun noise(label: str): i32 {{
            print(label);
            0
        }}

        fun main() {{
            let total: Boxy<i32> = boxed(\"L\", 40)? + noise(\"M\") + boxed(\"R\", 2)?;
            print(i\"{{format(total.value)}} [{{total.tag}}]\");
        }}
        "
        ),
        "L\nM\nR\n42 [L+R.map]\n",
    );
}

#[test]
fn expression_lift_on_a_user_container_keeps_the_marker_as_the_gate() {
    // The opt-in gate is unchanged by the trait path: a mappable container
    // WITHOUT `impl .. with Lift` is still refused, and the message names
    // the marker (the same steer `?.` gives).
    assert_fails_with(
        r#"
        struct Sneaky<T> { value: T }
        impl Sneaky<type T> {
            fun map<U>(self, fn: |T| U): Sneaky<U> {
                Sneaky { value = fn(self.value) }
            }
        }
        fun main() {
            let s = Sneaky { value = 1 };
            let x = s? + 1;
        }
        "#,
        "opting in with `impl .. with Lift`",
    );
}

#[test]
fn expression_lift_on_a_user_container_names_the_missing_contract_member() {
    // `Lift` is a MARKER — no members, so B29's conformance check has
    // nothing to verify. The duck-typed lookup is the contract's real gate:
    // a container with only `map`, used where the body flattens, is told
    // which member the contract wants.
    assert_fails_with(
        r#"
        import std::operators::Lift;
        struct Halfy<T> { value: T }
        impl Halfy<type T> with Lift {}
        impl Halfy<type T> {
            fun map<U>(self, fn: |T| U): Halfy<U> {
                Halfy { value = fn(self.value) }
            }
        }
        fun main() {
            let rows: Halfy<List<Halfy<i32>>> = Halfy { value = [Halfy { value = 7 }] };
            let first: Halfy<i32> = rows?[0];
        }
        "#,
        "needs an `and_then` method: the Lift contract",
    );
}

#[test]
fn expression_lift_on_a_user_container_rejects_a_mixed_region() {
    // "All receivers must be the same named container" holds across the
    // std/user line too, and the message names both rather than assuming
    // the Option/Result pair.
    assert_fails_with(
        &format!(
            "{USER_LIFT_CONTAINER}
        import std::option::Option::{{ self, Some, None }};
        fun main() {{
            let boxed = Boxy {{ value = 40, tag = \"a\" }};
            let counted = Some(2);
            let total = boxed? + counted?;
        }}
        "
        ),
        "must split the same container",
    );
}

#[test]
fn expression_lift_never_absorbs_a_user_container_chain() {
    // §5's absorption rejection is container-agnostic: a `?.` chain stays a
    // sealed atom, so `boxed?.name` is still `Boxy<str>` — the region shipping
    // for user containers does not change what a chain means.
    assert_compiles_and_runs(
        &format!(
            "{USER_LIFT_CONTAINER}
        struct User {{ name: str }}
        fun main() {{
            let boxed = Boxy {{ value = User {{ name = \"ada\" }}, tag = \"u\" }};
            let named = boxed?.name;
            print(i\"{{named.value}} [{{named.tag}}]\");
        }}
        "
        ),
        "ada [u.map]\n",
    );
}

// The primitive operator/equality impls: generic `T: Add`/`T: BitAnd` code
// dispatches to the numeric primitives (and `str` for Add), and the bodies
// lower to the native operators — including u32's `>>> 0` correction.
#[test]
fn primitive_operator_impls_dispatch_generically() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::operators::{ Add, BitAnd };

        fun sum<T: Add>(a: T, b: T): T {
        	a.add(b)
        }

        fun low_bit<T: BitAnd>(value: T, one: T): T {
        	value.bit_and(one)
        }

        fun main() {
        	print(format(sum(40, 2)));
        	print(sum("con", "cat"));
        	print(format(sum(1.5, 2.25)));
        	print(sum(20n, 22n));
        	print(format(low_bit(7, 1)));
        	print(format(low_bit(8u32, 1u32)));
        }
        "#,
        "42\nconcat\n3.75\n42n\n1\n0\n",
    );
}

// `format` covers every displayable primitive — u32 and BigInt were silently
// missing (the bound dispatch emitted the abstract to_string → undefined).
#[test]
fn format_covers_u32_and_bigint() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;

        fun main() {
        	print(format(7u32));
        	print(format(42n));
        }
        "#,
        "7\n42\n",
    );
}

// --- Block-scoped imports (backlog H2) ---
// `import`/`use` are statements, legal in any block; a binding is visible
// throughout its enclosing block (like a `let`), shadows outer scopes, and
// compiles to nothing. The loader finds module references at any depth.

// The loader half: `std::io` is referenced ONLY inside the body, so the module
// must still enter the reachable set (collect_module_refs recurses).
#[test]
fn an_import_in_a_function_body_binds_and_loads_its_module() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            import std::io;
            io::print("from the body");
        }

        main();
        "#,
        "from the body\n",
    );
}

// Flat block scope, like a `let`: the binding is visible before its statement
// (imports have no runtime effect, so there is no TDZ hazard either).
#[test]
fn a_body_import_binds_throughout_its_block_like_a_let() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            io::print("early");
            import std::io;
        }

        main();
        "#,
        "early\n",
    );
}

// Confinement: a block's import is invisible outside the block. `outer` comes
// first so the failing `io` is the source's first occurrence (the span pin).
#[test]
fn a_body_import_is_confined_to_its_function() {
    assert_fails_spanning(
        r#"
        fun outer() {
            io::print("outer");
        }

        fun inner() {
            import std::io;
            io::print("inner");
        }

        fun main() {
            inner();
            outer();
        }

        main();
        "#,
        "io",
        "cannot find",
    );
}

#[test]
fn an_inner_block_import_is_confined_to_the_block() {
    assert_fails_spanning(
        r#"
        fun escaped() {
            io::print("outside");
        }

        fun main() {
            {
                import std::io;
                io::print("inner");
            }
            print("separator");
            escaped();
        }

        main();
        "#,
        "io",
        "cannot find",
    );
}

#[test]
fn an_import_inside_an_if_arm_works() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            if true {
                import std::io;
                io::print("then");
            } else {
                import std::io;
                io::print("else");
            }
        }

        main();
        "#,
        "then\n",
    );
}

#[test]
fn an_import_inside_a_match_arm_works() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            match 2 {
                2 => {
                    import std::io;
                    io::print("two");
                }
                _ => {}
            }
        }

        main();
        "#,
        "two\n",
    );
}

#[test]
fn an_import_inside_a_closure_body_works() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            let show = || {
                import std::io;
                io::print("from closure");
            };
            show();
        }

        main();
        "#,
        "from closure\n",
    );
}

// A function declared in the block resolves the block's import through the
// ordinary scope chain.
#[test]
fn a_nested_function_sees_its_blocks_import() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            import std::io;
            fun emit() {
                io::print("nested");
            }
            emit();
        }

        main();
        "#,
        "nested\n",
    );
}

// An impl body is a statement list too: an import there serves every method.
#[test]
fn an_import_inside_an_impl_body_serves_its_methods() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Greeter {
            name: str,
        }

        impl Greeter {
            import std::display::format;

            fun greet(self) {
                print(format(self.name));
            }
        }

        fun main() {
            let greeter = Greeter { name = "vi" };
            greeter.greet();
        }

        main();
        "#,
        "vi\n",
    );
}

// Scoped `use` rides the same machinery: an inner `use` shadows the outer
// binding for its block, and the outer one is restored after.
#[test]
fn a_scoped_use_shadows_and_restores() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        mod alpha {
            export fun tag(): str {
                "alpha"
            }
        }

        mod beta {
            export fun tag(): str {
                "beta"
            }
        }

        use alpha::tag;

        fun main() {
            print(tag());
            {
                use beta::tag;
                print(tag());
            }
            print(tag());
        }

        main();
        "#,
        "alpha\nbeta\nalpha\n",
    );
}

// A block-scoped binding is deliberately not exportable — and no other
// `export` means anything inside a body.
#[test]
fn an_export_inside_a_body_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            export import std::io;
        }

        main();
        "#,
        "export import std::io;",
        "`export` is a module-level item",
    );
}

// A body import of a module that does not exist fails at the import itself,
// not with a panic or a cascade at the use sites.
#[test]
fn a_body_import_of_a_missing_module_errors_cleanly() {
    assert_fails_spanning(
        r#"
        fun main() {
            import std::nonexistent;
        }

        main();
        "#,
        "nonexistent",
        "cannot find 'nonexistent' in the imported path",
    );
}

// --- The macro engine, Phase 1 (macro-engine.md §3-§4) ---
// `macro fun` definitions compile hermetically per file and run in the
// expansion interpreter; `[name(args)]` and `[derive(Name)]` splice their
// returned Source before analysis.

// B194's reflection surface: a struct's own GENERIC PARAMETERS reach a macro —
// names, written bounds, and defaults — plus the two spellings every derive
// generator needs from them. Without these a generator can only name its
// subject bare, which is an under-supplied application (B188).
#[test]
fn a_macro_reads_its_subjects_generic_parameters() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::debug::Debug;

        macro fun report(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };
            import macro_std::build::{ impl_of, fun_of, quote, join };

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            mut described: List<str> = [];
            for parameter in target.generics {
                mut bounds: List<str> = [];
                for bound in parameter.bounds {
                    bounds.push(bound.render());
                }
                described.push(parameter.name + "/" + join(bounds, "+") + "/" + parameter.default_);
            }
            let binder_list = target.binders("Debug");
            let reporter = fun_of("report")
                .parameter("self")
                .returns("str")
                .expr(quote(target.subject() + " " + binder_list + " " + join(described, " ")));
            source(impl_of(target.name).generics(binder_list).member(reporter.render()).render())
        }

        // `K` is reached (a field is typed by it) and carries a written bound
        // and no default; `P` is phantom and carries a default and no bound.
        [report]
        struct Pack<K: Debug, P = i32> {
            key: K,
            count: i32,
        }

        fun main() {
            print(Pack { key = 1, count = 2 }.report());
        }

        main();
        "#,
        "Pack<K, P> <type K: Debug, type P> K/Debug/ P//i32\n",
    );
}

// The whole pipeline: hermetic world compile, attribute dispatch, reflection,
// interpreter run, splice, and dispatch INTO the generated impl.
#[test]
fn a_macro_attribute_expands_and_the_generated_impl_dispatches() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::{ Display, format };

        macro fun derive_display(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            mut arms = "";
            mut first = true;
            for field in target.fields {
                if first {
                    first = false;
                } else {
                    arms = arms + " + \", \" + ";
                }
                arms = arms + "\"" + field.name + "=\" + format(self." + field.name + ")";
            }
            source(
                "impl " + target.name + " with Display {\n"
                    + "fun to_string(self): str {\n"
                    + "import std::display::format;\n"
                    + arms + "\n}\n}\n",
            )
        }

        [derive_display]
        struct Point {
            x: i32,
            y: i32,
        }

        fun main() {
            print(format(Point { x = 1, y = 2 }));
        }

        main();
        "#,
        "x=1, y=2\n",
    );
}

// `[derive(Name)]` dispatches to a registered macro named `Name`; built-in
// derive names keep their Rust generators.
#[test]
fn a_derive_name_dispatches_to_a_registered_macro() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun Tagged(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            source("impl " + target.name + " {\nfun tag(self): str {\n\"" + target.name + "\"\n}\n}\n")
        }

        [derive(Tagged)]
        struct Widget {
            size: i32,
        }

        fun main() {
            print(Widget { size = 3 }.tag());
        }

        main();
        "#,
        "Widget\n",
    );
}

// A two-parameter macro receives the invocation's argument SOURCE TEXTS.
#[test]
fn a_macro_receives_its_arguments_as_source_text() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun labelled(item: Item, arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, Arguments, StructItem };
            import macro_std::option::Option::{ self, Some, None };

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            mut body = "";
            mut first = true;
            for value in arguments.values {
                if first {
                    first = false;
                    // A string argument arrives with its quotes — a ready
                    // expression to splice.
                    body = value;
                } else {
                    body = body + " + format(" + value + ")";
                }
            }
            source(
                "impl " + target.name + " {\nfun label(self): str {\n"
                    + "import std::display::format;\n" + body + "\n}\n}\n",
            )
        }

        [labelled("alpha-", 42)]
        struct Thing {
            n: i32,
        }

        fun main() {
            print(Thing { n = 1 }.label());
        }

        main();
        "#,
        "alpha-42\n",
    );
}

// A macro's output can itself carry a built-in derive — the expansion fixpoint.
#[test]
fn a_macros_output_can_carry_a_builtin_derive() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun make_pair(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            import macro_std::meta::{ Item, Source };

            source("[derive(PartialEq)]\nstruct Pair {\na: i32,\nb: i32,\n}\n")
        }

        [make_pair]
        struct Seed {
            unused: i32,
        }

        fun main() {
            let left = Pair { a = 1, b = 2 };
            let same = Pair { a = 1, b = 2 };
            let different = Pair { a = 9, b = 2 };
            print(left == same);
            print(left == different);
        }

        main();
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn an_unknown_macro_attribute_errors_cleanly() {
    assert_fails_spanning(
        r#"
        [no_such_macro]
        struct Point {
            x: i32,
        }

        fun main() {}

        main();
        "#,
        "no_such_macro",
        "no macro named `no_such_macro` is in scope",
    );
}

// Hermeticity (§4): a macro body may import only from `macro_std`.
#[test]
fn a_macro_body_importing_std_is_rejected() {
    assert_fails_spanning(
        r#"
        macro fun bad(item: Item): Source {
            import std::io;
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("")
        }

        fun main() {}

        main();
        "#,
        "import std::io",
        "a macro body may import only from `macro_std`",
    );
}

// A panic inside a macro surfaces as a spanned failure at the invocation.
#[test]
fn a_macro_panic_surfaces_at_the_invocation() {
    assert_fails_spanning(
        r#"
        [explode]
        struct Point {
            x: i32,
        }

        macro fun explode(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            import macro_std::panic;
            panic("unsupported item shape");
            source("")
        }

        fun main() {}

        main();
        "#,
        "explode",
        "failed at expansion time",
    );
}

#[test]
fn a_macro_generating_invalid_vilan_errors_at_the_site() {
    assert_fails_spanning(
        r#"
        [broken]
        struct Point {
            x: i32,
        }

        macro fun broken(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("fun {")
        }

        fun main() {}

        main();
        "#,
        "broken",
        "generated invalid Vilan",
    );
}

#[test]
fn a_macro_generating_a_macro_is_rejected() {
    assert_fails_spanning(
        r#"
        [sneaky]
        struct Point {
            x: i32,
        }

        macro fun sneaky(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("macro fun nested(item: Item): Source {\nimport macro_std::source;\nsource(\"\")\n}\n")
        }

        fun main() {}

        main();
        "#,
        "sneaky",
        "macros cannot define macros",
    );
}

#[test]
fn duplicate_macro_names_error() {
    assert_fails(
        r#"
        macro fun twice(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("")
        }

        macro fun twice(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("")
        }

        fun main() {}

        main();
        "#,
    );
}

// The fuel budget bounds a runaway macro (§5): the failure names the macro at
// its invocation instead of hanging the compiler.
#[test]
fn an_infinite_macro_is_stopped_by_fuel() {
    assert_fails_spanning(
        r#"
        [forever]
        struct Point {
            x: i32,
        }

        macro fun forever(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            mut n = 0;
            for {
                n = n + 1;
            }
            source("")
        }

        fun main() {}

        main();
        "#,
        "forever",
        "failed at expansion time",
    );
}

#[test]
fn a_macro_fun_inside_a_body_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            macro fun inner(item: Item): Source {
                import macro_std::source;
                import macro_std::meta::{ Item, Source };
                source("")
            }
        }

        main();
        "#,
        "macro fun inner(item: Item): Source {
                import macro_std::source;
                import macro_std::meta::{ Item, Source };
                source(\"\")
            }",
        "must be a top-level item",
    );
}

// --- The macro engine, Phase 2: `macro name(args)` invocations ---

#[test]
fn an_item_invocation_stamps_out_declarations() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun constants(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::meta::{ Arguments, Source };

            mut body = "";
            mut index = 0;
            for name in arguments.values {
                body = body + i"fun {name}(): i32 \{ {index} \}\n";
                index = index + 1;
            }
            source(body)
        }

        macro constants(zero, one, two);

        fun main() {
            print(two());
            print(zero());
        }

        main();
        "#,
        "2\n0\n",
    );
}

#[test]
fn an_expression_invocation_splices_in_place() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun double_of(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::meta::{ Arguments, Source };
            import macro_std::option::Option::{ self, Some, None };

            let text = match arguments.values.get(0) {
                Some(let value) => value,
                None => "0",
            };
            source(i"(({text}) * 2)")
        }

        fun main() {
            print(macro double_of(21));
            print(1 + macro double_of(3 + 4));
        }

        main();
        "#,
        "42\n15\n",
    );
}

// A zero-parameter macro is invocable with empty parens.
#[test]
fn a_unit_macro_invokes_with_no_arguments() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun answer(): Source {
            import macro_std::source;
            import macro_std::meta::Source;

            source("42")
        }

        fun main() {
            print(macro answer());
        }

        main();
        "#,
        "42\n",
    );
}

// Gensym hygiene (§7): `fresh()` placeholders stamp unique per splice site, so
// one macro's output cannot capture a binder another site introduced.
#[test]
fn gensyms_do_not_capture_across_splice_sites() {
    assert_fails(
        r#"
        macro fun binds(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::fresh;
            import macro_std::meta::{ Arguments, Source };

            let binder = fresh();
            source(i"\{ let {binder} = 1; {binder} + macro leaks() \}")
        }

        macro fun leaks(): Source {
            import macro_std::source;
            import macro_std::fresh;
            import macro_std::meta::Source;

            // Emits a REFERENCE to its own fresh placeholder without binding
            // it: if stamping were per-program instead of per-site, this would
            // silently capture `binds`'s binder.
            source(i"{fresh()}")
        }

        fun main() {
            let x = macro binds();
        }

        main();
        "#,
    );
}

// An item-position macro whose output carries a `fresh()` gensym: the stamped
// ITEM path (`macros.rs` `Some(stamped) => parse_cached`), which the corpus
// exercises only in expression position. Content-keying the stamped parse
// (analysis-reuse.md §2) must keep it working — the stamped name both binds a
// declaration and is referenced from a second one within the same expansion.
#[test]
fn an_item_invocation_with_a_gensym_binds_and_references_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun genfun(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::fresh;
            import macro_std::meta::{ Arguments, Source };

            let name = fresh();
            source(i"fun {name}(): i32 \{ 42 \}\nfun caller(): i32 \{ {name}() \}")
        }

        macro genfun();

        fun main() {
            print(caller());
        }

        main();
        "#,
        "42\n",
    );
}

// Shape mismatches are clean errors in both directions.
#[test]
fn an_attribute_shaped_macro_cannot_be_invoked() {
    assert_fails_spanning(
        r#"
        fun main() {
            let x = macro takes_item();
        }

        macro fun takes_item(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source("")
        }

        main();
        "#,
        "takes_item",
        "attribute-shaped",
    );
}

#[test]
fn an_invocation_shaped_macro_cannot_be_an_attribute() {
    assert_fails_spanning(
        r#"
        [takes_arguments]
        struct Point {
            x: i32,
        }

        macro fun takes_arguments(arguments: Arguments): Source {
            import macro_std::source;
            import macro_std::meta::{ Arguments, Source };
            source("")
        }

        fun main() {}

        main();
        "#,
        "takes_arguments",
        "invocation-shaped",
    );
}

// An expression splice must be exactly one expression.
#[test]
fn an_expression_macro_must_generate_one_expression() {
    assert_fails_spanning(
        r#"
        fun main() {
            let x = macro two_statements();
        }

        macro fun two_statements(): Source {
            import macro_std::source;
            import macro_std::meta::Source;
            source("1; 2;")
        }

        main();
        "#,
        "two_statements",
        "generated invalid Vilan",
    );
}

// B13, FIXED: a direct call on a let-bound closure now fills an unannotated
// parameter's shared type slot from the argument, so the body's uses type.
// (The first call site wins; later calls compare against it.)
#[test]
fn a_direct_call_types_an_unannotated_closure_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun accumulate(i: i32): i32 {
            i * 10
        }

        fun main() {
            let f = |i| accumulate(i);
            print(f(3));
        }

        main();
        "#,
        "30\n",
    );
}

// `str.code_at` — the UTF-16 code-unit accessor (added for the service
// macro's djb2 contract hash; charCodeAt under the hood).
#[test]
fn code_at_reads_utf16_units() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print("A".code_at(0));
            print("ab".code_at(1));
        }

        main();
        "#,
        "65\n98\n",
    );
}

// --- Scoped macro names (macro-engine.md §3 — the flat namespace is gone) ---

// A macro in another module needs a leaf import; unimported = a clean error.
#[test]
fn an_unimported_macro_from_another_module_is_not_in_scope() {
    assert_fails_spanning(
        r#"
        [tag]
        struct Point {
            x: i32,
        }

        mod helpers {
            macro fun tag(item: Item): Source {
                import macro_std::source;
                import macro_std::meta::{ Item, Source };
                source("")
            }
        }

        fun main() {}

        main();
        "#,
        "tag",
        "no macro named `tag` is in scope",
    );
}

// A user macro may now SHADOW a prelude derive for its own file — the
// reserved-name rule died with the flat namespace.
#[test]
fn a_user_macro_shadows_a_prelude_derive_in_its_file() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun PartialEq(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            source(i"impl {target.name} \{\nfun shadowed(self): str \{\n\"local\"\n\}\n\}\n")
        }

        [derive(PartialEq)]
        struct Point {
            x: i32,
        }

        fun main() {
            print(Point { x = 1 }.shadowed());
        }

        main();
        "#,
        "local\n",
    );
}

// The prelude: `[derive(PartialEq)]` still needs no import — the derive
// macros live in always-loaded std modules now, not in a special file.
#[test]
fn prelude_derives_need_no_import() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        [derive(PartialEq)]
        struct Point {
            x: i32,
        }

        fun main() {
            let a = Point { x = 1 };
            let b = Point { x = 1 };
            print(a == b);
        }

        main();
        "#,
        "true\n",
    );
}

// --- I4: `List<T: PartialEq>` implements `PartialEq` (element-wise, length
// first). Found by E80's lane: `[derive(PartialEq)]` on a struct with a
// `List<…>` field was refused ("type 'List' does not implement the PartialEq
// operator"), and the website's `DiagRow` wrote its `eq` by hand. One pin per
// case; the derive pin is the shape that was refused, and removing the impl
// reddens every one of them with that exact refusal — the plant IS the old
// state.

#[test]
fn i4_empty_lists_are_equal() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let a: List<i32> = [];
            let b: List<i32> = [];
            print(a == b);
        }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn i4_equal_lists_compare_equal_through_both_spellings() {
    // `==` is what the derive emits per field; `.eq` is the trait member the
    // generic `T: PartialEq` world calls — same impl, both pinned.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let a = [1, 2, 3];
            let b = [1, 2, 3];
            print(a == b);
            print(a.eq(b));
            print(a != b);
        }
        main();
        "#,
        "true\ntrue\nfalse\n",
    );
}

#[test]
fn i4_lists_of_unequal_length_are_not_equal() {
    // Length first: a strict prefix is not equal, in either direction — the
    // shorter side never indexes past its end.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print(["a", "b"] == ["a", "b", "c"]);
            print(["a", "b", "c"] == ["a", "b"]);
        }
        main();
        "#,
        "false\nfalse\n",
    );
}

#[test]
fn i4_lists_with_a_differing_element_are_not_equal() {
    // Same length, one pair disagrees — first, middle, and last position.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print([9, 2, 3] == [1, 2, 3]);
            print([1, 9, 3] == [1, 2, 3]);
            print([1, 2, 9] == [1, 2, 3]);
        }
        main();
        "#,
        "false\nfalse\nfalse\n",
    );
}

#[test]
fn i4_nested_lists_compare_element_wise() {
    // The impl is conditional (`T: PartialEq`), so `List<i32>: PartialEq`
    // makes `List<List<i32>>: PartialEq` — equality recurses through the
    // element impl, and a deep disagreement surfaces.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            print([[1, 2], [3]] == [[1, 2], [3]]);
            print([[1, 2], [3]] == [[1, 2], [4]]);
            print([[1, 2], [3]] == [[1, 2]]);
        }
        main();
        "#,
        "true\nfalse\nfalse\n",
    );
}

#[test]
fn i4_a_struct_with_a_list_field_derives_partial_eq() {
    // The refused shape itself (E80's finding, the website's `DiagRow`): the
    // derive emits `self.tags == other.tags`, which lands on the new impl.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        [derive(PartialEq)]
        struct Row {
            id: i32,
            tags: List<str>,
        }

        fun main() {
            let a = Row { id = 1, tags = ["x", "y"] };
            let b = Row { id = 1, tags = ["x", "y"] };
            let c = Row { id = 1, tags = ["x"] };
            print(a == b);
            print(a == c);
        }
        main();
        "#,
        "true\nfalse\n",
    );
}

// The macro world's AMBIENT meta prelude (macro-engine.md §3/§10): the
// reflection vocabulary — the meta types, `source`, `fresh` — is in scope in
// every macro body with no imports at all. Libraries (`option`, `build`)
// stay explicit.
#[test]
fn the_meta_vocabulary_is_ambient_in_macro_bodies() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun tag(item: Item): Source {
            import macro_std::option::Option::{ self, Some, None };

            let name = match item.as_struct() {
                Some(let found) => found.name,
                None => "?",
            };
            source(i"fun tag_of(): str \{\n\"{name}\"\n\}\n")
        }

        [tag]
        struct Widget {
            size: i32,
        }

        fun main() {
            print(tag_of());
        }

        main();
        "#,
        "Widget\n",
    );
}

// `fresh()` is part of the ambient vocabulary too — a zero-import invocation
// macro gensyms and splices.
#[test]
fn fresh_is_ambient_in_macro_bodies() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun doubled(arguments: Arguments): Source {
            let slot = fresh();
            source(i"let {slot} = 21;\nlet answer = {slot} + {slot};")
        }

        macro doubled()

        fun main() {
            print(answer);
        }

        main();
        "#,
        "42\n",
    );
}

// An explicit same-name definition SHADOWS the ambient prelude — the prelude
// binds first, ordinary resolution order.
#[test]
fn a_macro_fun_shadows_the_ambient_prelude() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun fresh(): str {
            "__custom"
        }

        macro fun emit(arguments: Arguments): Source {
            let slot = fresh();
            source(i"fun generated(): str \{\n\"{slot}\"\n\}\n")
        }

        macro emit()

        fun main() {
            print(generated());
        }

        main();
        "#,
        "__custom\n",
    );
}

// --- `macro { .. }` blocks (macro-engine.md Phase 4) ---

// ITEM position: the block's emissions splice as items.
#[test]
fn an_item_position_macro_block_splices_items() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro {
            source("fun answer(): i32 {\n42\n}\n")
        }

        fun main() {
            print(answer());
        }

        main();
        "#,
        "42\n",
    );
}

// EXPRESSION position: the block folds at compile time and splices one
// expression.
#[test]
fn an_expression_position_macro_block_splices_an_expression() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let folded = macro {
                mut total = 0;
                mut index = 1;
                for index <= 4 {
                    total = total + index;
                    index = index + 1;
                }
                source(i"{total}")
            };
            print(folded);
        }

        main();
        "#,
        "10\n",
    );
}

// A block calls the file's `macro fun` helpers as plain in-world functions.
#[test]
fn a_macro_block_calls_a_same_file_helper() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun doubled(value: i32): str {
            i"{value * 2}"
        }

        fun main() {
            print(macro { source(doubled(21)) });
        }

        main();
        "#,
        "42\n",
    );
}

// The synthetic entry declares `: Source`, so a non-Source tail is a world
// type error at the block's true position.
#[test]
fn a_macro_block_must_yield_source() {
    assert_fails_spanning(
        r#"
fun main() {
    let x = macro { 42 };
}

main();
        "#,
        "macro { 42 }",
        "definition did not compile",
    );
}

// Output that doesn't parse is the ordinary invalid-vilan error, with the
// block's own label.
#[test]
fn a_macro_block_with_invalid_output_errors() {
    assert_fails_spanning(
        r#"
fun main() {
    let x = macro { source("+++ nope") };
}

main();
        "#,
        r#"macro { source("+++ nope") }"#,
        "generated invalid Vilan",
    );
}

// Inside a `macro fun` body there is nothing to splice into — the body
// already runs at expansion time.
#[test]
fn a_macro_block_inside_a_macro_fun_is_rejected() {
    assert_fails_spanning(
        r#"
macro fun bad(item: Item): Source {
    macro { source("1") }
}

fun main() {}

main();
        "#,
        r#"macro { source("1") }"#,
        "cannot appear inside macro code",
    );
}

// Same rule one level down: blocks cannot nest.
#[test]
fn a_macro_block_inside_a_macro_block_is_rejected() {
    assert_fails_spanning(
        r#"
fun main() {
    let x = macro { macro { source("1") } };
}

main();
        "#,
        r#"macro { source("1") }"#,
        "cannot appear inside macro code",
    );
}

// Block bodies are hermetic like every macro body: imports root at
// `macro_std` only.
#[test]
fn a_macro_block_body_is_hermetic() {
    assert_fails_spanning(
        r#"
fun main() {
    let x = macro {
        import std::io::print;
        source("1")
    };
}

main();
        "#,
        "import std::io::print",
        "hermetic",
    );
}

// A macro's output cannot carry a `macro { .. }` block (mirrors the
// macro-generating-macro rejection).
#[test]
fn generated_code_cannot_carry_a_macro_block() {
    let source = r#"
macro fun emit_block(arguments: Arguments): Source {
    source("fun answer(): i32 {\nmacro { source(\"1\") }\n}\n")
}

macro emit_block()

fun main() {}

main();
        "#;
    let diagnostics = failure_diagnostics(source);
    // The error anchors at the GENERATING invocation's name (a file span),
    // never into the generated text.
    let invocation_name = source.rfind("emit_block").unwrap();
    assert!(
        diagnostics.iter().any(|(message, range)| {
            message.contains("generated a `macro { .. }` block") && range.start == invocation_name
        }),
        "expected the generated-block rejection at the invocation; got: {diagnostics:#?}"
    );
}

// --- Sized numeric types (proposal/numeric-types.md) ---

// Every new suffix types its literal; `128i8` is admitted (the minimum is
// written as unary minus over the literal); unsuffixed literals adopt an
// expected sized type.
#[test]
fn sized_numeric_literals_type_and_run() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let a = 5i8;
            let b = 200u8;
            let c = 5i16;
            let d = 60000u16;
            let e = 5i53;
            let f = 5u53;
            let g = 2.5f32;
            let allowed = 128i8;
            let expected: u8 = 7;
            let fractional: f32 = 1.5;
            print(a + a);
            print(b);
            print(c + c);
            print(d);
            print(e + f.as_i53());
            print(g);
            print(allowed);
            print(expected);
            print(fractional);
        }

        main();
        "#,
        "10\n200\n10\n60000\n10\n2.5\n128\n7\n1.5\n",
    );
}

#[test]
fn a_u8_literal_out_of_range_errors() {
    assert_fails_spanning(
        "fun main() { let x = 300u8; }\nmain();\n",
        "300u8",
        "out of range for `u8` (0 ..= 255)",
    );
}

#[test]
fn an_i8_literal_out_of_range_errors() {
    assert_fails_spanning(
        "fun main() { let x = 129i8; }\nmain();\n",
        "129i8",
        "out of range for `i8` (-128 ..= 127)",
    );
}

#[test]
fn a_u16_literal_out_of_range_errors() {
    assert_fails_spanning(
        "fun main() { let x = 70000u16; }\nmain();\n",
        "70000u16",
        "out of range for `u16`",
    );
}

#[test]
fn an_i16_literal_out_of_range_errors() {
    assert_fails_spanning(
        "fun main() { let x = 40000i16; }\nmain();\n",
        "40000i16",
        "out of range for `i16`",
    );
}

#[test]
fn a_u32_literal_out_of_range_errors() {
    assert_fails_spanning(
        "fun main() { let x = 5000000000u32; }\nmain();\n",
        "5000000000u32",
        "out of range for `u32`",
    );
}

#[test]
fn an_i32_literal_out_of_range_errors() {
    assert_fails_spanning(
        "fun main() { let x = 3000000000i32; }\nmain();\n",
        "3000000000i32",
        "out of range for `i32`",
    );
}

#[test]
fn an_i53_literal_beyond_the_f64_window_errors() {
    assert_fails_spanning(
        "fun main() { let x = 9007199254740993i53; }\nmain();\n",
        "9007199254740993i53",
        "use `BigInt` for larger values",
    );
}

#[test]
fn a_hex_literal_is_range_checked() {
    assert_fails_spanning(
        "fun main() { let x = 0x100u8; }\nmain();\n",
        "0x100u8",
        "out of range for `u8`",
    );
}

// An unsuffixed literal adopting an expected sized type is range-checked
// against that type.
#[test]
fn an_expected_type_literal_is_range_checked() {
    assert_fails_spanning(
        "fun main() { let x: u8 = 300; }\nmain();\n",
        "300",
        "out of range for `u8`",
    );
}

// --- Type bounds: `max_value()` / `min_value()` ---
//
// The niladic-function stopgap for `i32::MAX` (vilan has no associated-const
// mechanism for one to hang on). A hand-transcribed bounds table is worth only
// what cross-checks it, so the pins come in three layers: the sixteen values
// spelled out, each value equal to the literal the compiler admits for that
// type, and the analyzer's own out-of-range diagnostic naming the same two
// numbers back.

#[test]
fn type_bounds_are_the_documented_per_type_ranges() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print(i8::max_value());
            print(i8::min_value());
            print(u8::max_value());
            print(u8::min_value());
            print(i16::max_value());
            print(i16::min_value());
            print(u16::max_value());
            print(u16::min_value());
            print(i32::max_value());
            print(i32::min_value());
            print(u32::max_value());
            print(u32::min_value());
            print(i53::max_value());
            print(i53::min_value());
            print(u53::max_value());
            print(u53::min_value());
        }
        main();
        "#,
        "127\n-128\n255\n0\n32767\n-32768\n65535\n0\n2147483647\n-2147483648\n\
         4294967295\n0\n9007199254740992\n-9007199254740992\n9007199254740992\n0\n",
    );
}

// Each bound is exactly the literal the compiler admits for that type — the
// pin that makes the table trustworthy rather than merely transcribed.
#[test]
fn type_bounds_equal_the_literals_the_compiler_admits() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print(i8::max_value() == 127i8);
            print(i8::min_value() == -128i8);
            print(u8::max_value() == 255u8);
            print(u8::min_value() == 0u8);
            print(i16::max_value() == 32767i16);
            print(i16::min_value() == -32768i16);
            print(u16::max_value() == 65535u16);
            print(u16::min_value() == 0u16);
            print(i32::max_value() == 2147483647i32);
            print(i32::min_value() == -2147483648i32);
            print(u32::max_value() == 4294967295u32);
            print(u32::min_value() == 0u32);
            print(i53::max_value() == 9007199254740992i53);
            print(i53::min_value() == -9007199254740992i53);
            print(u53::max_value() == 9007199254740992u53);
            print(u53::min_value() == 0u53);
        }
        main();
        "#,
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n\
         true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
    );
}

// The other direction: the analyzer's rendered range must read exactly
// `min_value() ..= max_value()`. If a bound in `number.vl` and the `BOUNDS`
// table in the analyzer ever drift apart, one of these two goes red.
#[test]
fn the_out_of_range_diagnostic_names_the_shipped_bounds() {
    assert_fails_spanning(
        "fun main() { let x = 129i8; }\nmain();\n",
        "129i8",
        "out of range for `i8` (-128 ..= 127)",
    );
    assert_fails_spanning(
        "fun main() { let x = 256u8; }\nmain();\n",
        "256u8",
        "out of range for `u8` (0 ..= 255)",
    );
    assert_fails_spanning(
        "fun main() { let x = 32769i16; }\nmain();\n",
        "32769i16",
        "out of range for `i16` (-32768 ..= 32767)",
    );
    assert_fails_spanning(
        "fun main() { let x = 65536u16; }\nmain();\n",
        "65536u16",
        "out of range for `u16` (0 ..= 65535)",
    );
    assert_fails_spanning(
        "fun main() { let x = 2147483649i32; }\nmain();\n",
        "2147483649i32",
        "out of range for `i32` (-2147483648 ..= 2147483647)",
    );
    assert_fails_spanning(
        "fun main() { let x = 4294967296u32; }\nmain();\n",
        "4294967296u32",
        "out of range for `u32` (0 ..= 4294967295)",
    );
}

// The wide pair's window is the symmetric ±2^53 (spec/lexical.md), so one past
// `u53::max_value()` is refused. `i53`'s counterpart is
// `an_i53_literal_beyond_the_f64_window_errors` above.
#[test]
fn a_u53_literal_past_the_window_errors() {
    assert_fails_spanning(
        "fun main() { let x = 9007199254740993u53; }\nmain();\n",
        "9007199254740993u53",
        "use `BigInt` for larger values",
    );
}

// The signed literal check admits the MAGNITUDE `2^(n-1)` so that the minimum
// can be written as unary minus over a literal (`-128i8`) — numeric-types.md
// §3's documented looseness. So `128i8` compiles while exceeding the type's
// maximum: `max_value()` is the TYPE's bound, deliberately not "the largest
// literal that compiles". Pinned so the pair is never "corrected" to match.
#[test]
fn the_signed_literal_looseness_reaches_one_past_max_value() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print(128i8 > i8::max_value());
            print(32768i16 > i16::max_value());
            print(2147483648i32 > i32::max_value());
        }
        main();
        "#,
        "true\ntrue\ntrue\n",
    );
}

// Integer division truncates toward zero (numeric-types.md §2) — both signs,
// every width, the compound form, and generic `T: Div` dispatch; float and
// BigInt division are untouched.
#[test]
fn integer_division_truncates_toward_zero() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Div;

        fun halve<T: Div>(value: T, divisor: T): T {
            value / divisor
        }

        fun main() {
            print(7 / 2);
            print(-7 / 2);
            print(7u32 / 2u32);
            print(100u8 / 3u8);
            print(100i53 / 8i53);
            mut compound = 9;
            compound /= 2;
            print(compound);
            print(halve(100i16, 8i16));
            print(7.0 / 2.0);
            print(7n / 2n);
        }

        main();
        "#,
        "3\n-3\n3\n33\n12\n4\n12\n3.5\n3n\n",
    );
}

#[test]
fn generic_numeric_operators_apply_their_verdict_for_every_width() {
    // A generic `T: Div`/`T: Shr` monomorphized to a native-JS width (`i32`/`u32`)
    // took an INLINE fast path in the transformer that dropped the per-instantiation
    // numeric verdict — division without `Math.trunc` (`7/2 == 3.5`), a `u32` shift
    // with the signed `>>` instead of `>>>`. Root cause: the recorded generic-lhs is
    // the bound's id (`Trait(Div)`), not a `Generic(..)` wrapper, so `resolve_type_id`
    // left it untouched; `resolve_constraint` now looks it up in the substitution.
    // Every other width was correct only because it DISPATCHED to its `number.vl`
    // impl — the one prior generic-division pin used `i16`, so it hid `i32`/`u32`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::{ Div, Shr, BitAnd };
        fun halve<T: Div>(v: T, d: T): T { v / d }
        fun shift<T: Shr>(v: T, by: T): T { v >> by }
        fun mask<T: BitAnd>(v: T, m: T): T { v & m }
        fun main() {
            print(halve(7i8, 2i8));      // 3
            print(halve(7i32, 2i32));    // 3 — was 3.5
            print(halve(9u32, 4u32));    // 2 — was 2.25
            print(halve(100i53, 8i53));  // 12
            print(shift(0x80000000u32, 1u32));  // 1073741824 — unsigned, was negative
            print(mask(0xF0u32, 0x3Cu32));      // 48
        }
        "#,
        "3\n3\n2\n12\n1073741824\n48\n",
    );
}

// Conversions carry Rust-`as` semantics: truncate toward zero, then fold
// two's-complement into the target's width.
#[test]
fn numeric_conversions_fold_into_the_target_width() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print((300).as_u8());
            print((-1).as_u8());
            print((130).as_i8());
            print((70000).as_u16());
            print((3.9).as_i32());
            print((-3.9).as_i32());
            print((200u8).as_f64() + 0.5);
            print((2.5f32).as_i53());
            print((5i53).as_u53());
        }

        main();
        "#,
        "44\n255\n-126\n4464\n3\n-3\n200.5\n2\n5\n",
    );
}

// The macro-engine flagship (macro-engine.md §2) realized: one macro stamps
// the operator family for several types at once. (The std family itself is
// generated-and-checked-in because `number.vl` loads inside macro worlds,
// which expand with an empty macro scope — world files must not dispatch.)
#[test]
fn a_macro_stamps_a_numeric_family() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        macro fun arithmetic_family(arguments: Arguments): Source {
            import macro_std::option::Option::{ self, Some, None };
            import macro_std::build::{ impl_of, fun_of };

            mut generated = "import std::operators::Add;\n";
            mut index = 0;
            for index < arguments.len() {
                let name = match arguments.as_identifier(index) {
                    Some(let found) => found,
                    None => "?",
                };
                let add = fun_of("add")
                    .parameter("self")
                    .parameter(i"b: {name}")
                    .returns(name)
                    .expr(i"{name} \{ value = self.value + b.value \}");
                generated = generated + impl_of(name).implements("Add").method(add).render();
                index = index + 1;
            }
            source(generated)
        }

        struct Meters { value: i32 }
        struct Seconds { value: i32 }

        macro arithmetic_family(Meters, Seconds)

        fun total<T: Add>(a: T, b: T): T {
            a + b
        }

        fun main() {
            print(total(Meters { value = 2 }, Meters { value = 3 }).value);
            print(total(Seconds { value = 40 }, Seconds { value = 5 }).value);
        }

        main();
        "#,
        "5\n45\n",
    );
}

// --- `flatten` + keyed reconciliation (backlog A4/A3) ---

// The join follows the CURRENT inner: switching detaches the replaced inner
// (its later sets must not leak through) and adopts the new one's value.
#[test]
fn flatten_follows_the_current_inner_and_detaches_the_old() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let first = Signal::new(1);
            let second = Signal::new(10);
            let outer = Signal::new(first);
            let joined = outer.flatten();
            first.set(2);
            print(joined.get());
            outer.set(second);
            first.set(99);
            print(joined.get());
            second.set(11);
            print(joined.get());
        }

        main();
        "#,
        "2\n10\n11\n",
    );
}

// Reconcile distinguishes keep/refresh/fresh per new position and reports
// removed old indices — including the duplicate-key claim rule.
#[test]
fn reconcile_plans_keep_refresh_fresh_and_removals() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ reconcile, RowStep };

        fun main() {
            let plan = reconcile([1, 2], [10, 20], [20, 11, 35, 20], |item| item / 10);
            for step in plan.steps {
                let rendered = match step {
                    RowStep::Keep(let index) => i"keep {index}",
                    RowStep::Refresh(let index) => i"refresh {index}",
                    RowStep::Fresh => "fresh",
                };
                print(rendered);
            }
            for index in plan.removed {
                print(i"removed {index}");
            }
        }

        main();
        "#,
        "keep 1\nrefresh 0\nfresh\nfresh\n",
    );
}

// `Owner.defer` runs plain cleanups at disposal, alongside taken disposables.
#[test]
fn owner_defer_runs_cleanups_on_dispose() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Disposable };

        fun main() {
            let owner = Owner::new();
            owner.defer(|| print("first"));
            owner.defer(|| print("second"));
            owner.dispose();
            print("done");
        }

        main();
        "#,
        "first\nsecond\ndone\n",
    );
}

// --- The ambient owner (proposal/ambient-owner.md, backlog A5) ---

// A covered `effect` registers into the ambient owner and dies with it.
#[test]
fn effect_registers_into_the_ambient_owner_and_dies_with_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope };

        fun main() {
            let count = Signal::new(1);
            let owner = Owner::new();
            owner_scope.run(owner, || {
                count.effect(|value| print(value));
            });
            count.set(2);
            owner.dispose();
            count.set(3);
            print("done");
        }

        main();
        "#,
        "1\n2\ndone\n",
    );
}

// The static fence: `effect` reachable outside every `owner_scope.run` is a
// compile error, not a runtime absence.
#[test]
fn effect_outside_an_owner_scope_is_a_compile_error() {
    let diagnostics = failure_diagnostics(
        r#"
import std::io::print;
import std::reactive::{ Signal, SignalCell };

fun main() {
    let count = Signal::new(1);
    count.effect(|value| print(value));
}
main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("without an enclosing `run`")),
        "expected the coverage fence; got: {diagnostics:#?}"
    );
}

// E74 (diagnostics-standard A2): the fence's strict read sits in STD when
// reached through `effect` (`get_owner`'s body, reactive.vl) — the diagnostic
// anchors at the USER'S call, with the std read demoted to the C3 note.
#[test]
fn e74_an_uncovered_effect_anchors_at_the_users_call() {
    let source = r#"
import std::io::print;
import std::reactive::{ Signal, SignalCell };

fun main() {
    let count = Signal::new(1);
    count.effect(|value| print(value));
}
main();
        "#;
    assert_fails_spanning(
        source,
        "count.effect(|value| print(value))",
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_only_failure_noting_into_std(
        source,
        "without an enclosing `run`",
        "the read is inside `get_owner` here",
    );
}

// E74's top-level entry: a module-level initializer calling straight into
// the std reader is an uncovered entry by construction, and the walk-back
// anchors at that initializer call (the `top_level_incoming` arm — there is
// no caller node to descend through).
#[test]
fn e74_a_module_initializer_entry_anchors_at_the_initializer_call() {
    let source = r#"
import std::io::print;
import std::reactive::{ Owner, get_owner };

let scope: Owner = get_owner();

fun main() {
    print("hi");
}
main();
        "#;
    assert_fails_spanning(
        source,
        "get_owner()",
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_only_failure_noting_into_std(
        source,
        "without an enclosing `run`",
        "the read is inside `get_owner` here",
    );
}

// E74's no-over-correction half: a strict read the user WROTE anchors at
// itself, with no std-frame note to demote.
#[test]
fn e74_a_direct_strict_read_still_anchors_at_itself() {
    let source = r#"
import std::io::print;
import std::reactive::owner_scope;

fun main() {
    let owner = owner_scope.get();
    print("hi");
}
main();
        "#;
    assert_fails_spanning(
        source,
        "owner_scope.get()",
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
    let diagnostics = failure_diagnostics_with_notes(source);
    assert!(
        diagnostics.iter().all(|(_, _, note)| note.is_none()),
        "a user-written read must not carry the std-frame note; got: {diagnostics:#?}"
    );
}

// E74's blame filter: the walk-back crosses only UNBOUND callers, so a
// covered `effect` earlier in the program (a lower call id, which the
// earliest-entry rule would otherwise prefer) is never blamed for the
// uncovered one beside it.
#[test]
fn e74_a_covered_call_beside_the_uncovered_one_is_not_blamed() {
    assert_fails_spanning(
        r#"
import std::io::print;
import std::reactive::{ Owner, Signal, SignalCell, owner_scope };

fun main() {
    let early = Signal::new(1);
    let owner = Owner::new();
    owner_scope.run(owner, || {
        early.effect(|value| print(value));
    });
    let late = Signal::new(2);
    late.effect(|value| print(value));
}
main();
        "#,
        "late.effect(|value| print(value))",
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
}

// --- E78: the coverage refusal keeps the chain the walk traverses. ---

/// The owner's acceptance example, comments as behavior: ONE diagnostic,
/// primary at the read (E74's anchor for a user-written read), with `b`'s
/// `a()` and `main`'s `b()` as trace labels ordered entry → read — and `c`'s
/// covered `context.run(0, || a())` carrying nothing (the exact-length
/// assertion is the covered-call stop's pin: mark the covered edge uncovered
/// in the trace walk and the extra label fails here).
#[test]
fn e78_the_owners_example_traces_the_uncovered_chain_and_leaves_the_covered_call_clean() {
    let source = r#"
import std::context::Context;

let context: Context<u32> = Context::new();

fun a() {
    context.get();
}

fun b() {
    a(); // error with trace
}

fun c() {
    context.run(0, || a()); // ok
}

fun main() {
    b(); // error with trace
    c();
}
        "#;
    assert_fails_once_with(
        source,
        "context `context` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_fails_spanning(
        source,
        "context.get()",
        "context `context` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_traces(
        source,
        "can be reached without an enclosing `run`",
        &[
            // Occurrence 1 skips each function's own declaration.
            ("b()", 1, "the context requirement flows through this call"),
            ("a()", 1, "the context requirement flows through this call"),
        ],
    );
}

/// A top-level call is an uncovered entry by construction, and it is a hop
/// like any other: appending `main();` to the owner's example adds exactly
/// one label, at the top-level call, ahead of the rest of the chain.
#[test]
fn e78_a_top_level_call_is_a_labeled_hop() {
    let source = r#"
import std::context::Context;

let context: Context<u32> = Context::new();

fun a() {
    context.get();
}

fun b() {
    a();
}

fun c() {
    context.run(0, || a());
}

fun main() {
    b();
    c();
}
main();
        "#;
    assert_traces(
        source,
        "can be reached without an enclosing `run`",
        &[
            (
                "main()",
                1,
                "the context requirement flows through this call",
            ),
            ("b()", 1, "the context requirement flows through this call"),
            ("a()", 1, "the context requirement flows through this call"),
        ],
    );
}

/// A two-hop chain through a capture: the closure's read blames its defining
/// scope's callers — the capture hop itself crosses no call site and adds no
/// label, so the chain is exactly the two calls.
#[test]
fn e78_a_chain_through_a_capture_labels_both_calls() {
    let source = r#"
import std::context::Context;

let context: Context<u32> = Context::new();

fun a() {
    let read = || context.get();
    read();
}

fun b() {
    a();
}

fun main() {
    b();
}
        "#;
    assert_fails_spanning(
        source,
        "context.get()",
        "context `context` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_traces(
        source,
        "can be reached without an enclosing `run`",
        &[
            ("b()", 1, "the context requirement flows through this call"),
            ("a()", 1, "the context requirement flows through this call"),
        ],
    );
}

/// A dispatch hop carries E74's union-admission residual and must not
/// overclaim: the site MAY select the reading implementation, so its label
/// says so, while the direct call above it keeps the plain wording.
#[test]
fn e78_a_dispatch_hop_says_may_flow() {
    let source = r#"
import std::io::print;
import std::context::Context;

let current: Context<i32> = Context::new();

trait Probe {
    fun name(self): str;

    fun report(self) {
        print(i"{self.name()}: {current.get()}");
    }
}

struct Widget { tag: str }

impl Widget with Probe {
    fun name(self): str {
        self.tag
    }
}

fun announce<T: Probe>(subject: T) {
    subject.report();
}

fun main() {
    announce(Widget { tag = "w" });
}
        "#;
    assert_traces(
        source,
        "can be reached without an enclosing `run`",
        &[
            (
                "announce(Widget { tag = \"w\" })",
                0,
                "the context requirement flows through this call",
            ),
            // The callee's own name, not the receiver chain (B229): a
            // dispatch hop takes the same anchor every method hop does.
            (
                "report",
                1,
                "the context requirement may flow through this call (dispatch may select a reader)",
            ),
        ],
    );
}

/// The cap: a nine-hop chain labels its six ENTRY-side hops — the outermost
/// frames, where the missing `run` belongs — and elides the read side behind
/// the honest tail, anchored at the last kept hop.
#[test]
fn e78_a_deep_chain_caps_at_six_labels_with_an_honest_tail() {
    let source = r#"
import std::context::Context;

let context: Context<u32> = Context::new();

fun f8() {
    context.get();
}
fun f7() { f8(); }
fun f6() { f7(); }
fun f5() { f6(); }
fun f4() { f5(); }
fun f3() { f4(); }
fun f2() { f3(); }
fun f1() { f2(); }

fun main() {
    f1();
}
main();
        "#;
    assert_traces(
        source,
        "can be reached without an enclosing `run`",
        &[
            (
                "main()",
                1,
                "the context requirement flows through this call",
            ),
            ("f1()", 1, "the context requirement flows through this call"),
            ("f2()", 1, "the context requirement flows through this call"),
            ("f3()", 1, "the context requirement flows through this call"),
            ("f4()", 1, "the context requirement flows through this call"),
            ("f5()", 1, "the context requirement flows through this call"),
            ("f5()", 1, "… 3 more uncovered calls on this path"),
        ],
    );
}

/// The covered-call stop, isolated: the read's function has two callers —
/// one inside `run`, one not — and only the uncovered one's chain labels.
/// This is the plant pin: treat the covered edge as uncovered in the trace
/// walk and the `|| a()` call gains a label the exact-length check refuses.
#[test]
fn e78_a_covered_caller_beside_the_open_path_is_never_labeled() {
    let source = r#"
import std::context::Context;

let context: Context<u32> = Context::new();

fun a() {
    context.get();
}

fun covered() {
    context.run(1, || a());
}

fun open_path() {
    a();
}

fun main() {
    covered();
    open_path();
}
        "#;
    assert_traces(
        source,
        "can be reached without an enclosing `run`",
        &[
            (
                "open_path()",
                1,
                "the context requirement flows through this call",
            ),
            ("a()", 2, "the context requirement flows through this call"),
        ],
    );
}

/// The `owner_scope` coverage flavor rides the same walk: the primary stays
/// at E74's anchor (the user's call entering std), the std read stays the C3
/// note, and the frames ABOVE the entry now label, entry → read.
#[test]
fn e78_the_std_read_chain_labels_the_frames_above_the_entry() {
    let source = r#"
import std::io::print;
import std::reactive::{ Signal, SignalCell };

fun watch(count: SignalCell<i32>) {
    count.effect(|value| print(value));
}

fun setup() {
    let count = Signal::new(1);
    watch(count);
}

fun main() {
    setup();
}
main();
        "#;
    assert_fails_spanning(
        source,
        "count.effect(|value| print(value))",
        "context `owner_scope` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_only_failure_noting_into_std(
        source,
        "without an enclosing `run`",
        "the read is inside `get_owner` here",
    );
    assert_traces(
        source,
        "can be reached without an enclosing `run`",
        &[
            (
                "main()",
                1,
                "the context requirement flows through this call",
            ),
            (
                "setup()",
                1,
                "the context requirement flows through this call",
            ),
            (
                "watch(count)",
                0,
                "the context requirement flows through this call",
            ),
        ],
    );
}

/// Several uncovered entries reaching one std read: each gets its OWN
/// primary and its own chain — E74 kept only the least-id entry, so fixing
/// the first call merely revealed the second on the next compile.
#[test]
fn e78_each_uncovered_entry_gets_its_own_diagnostic() {
    let source = r#"
import std::io::print;
import std::reactive::{ Signal, SignalCell };

fun main() {
    let first = Signal::new(1);
    first.effect(|value| print(value));
    let second = Signal::new(2);
    second.effect(|value| print(value));
}
main();
        "#;
    let message = "context `owner_scope` is read here, but this code can be reached without an enclosing `run`";
    let diagnostics = failure_diagnostics(source);
    let matching = diagnostics
        .iter()
        .filter(|(candidate, _)| candidate.contains(message))
        .count();
    assert_eq!(
        matching, 2,
        "one diagnostic per uncovered entry; got: {diagnostics:#?}"
    );
    assert_fails_spanning(source, "first.effect(|value| print(value))", message);
    assert_fails_spanning(source, "second.effect(|value| print(value))", message);
}

/// The injected-call flavor (row 223) traces through the same walk: the
/// uncovered `body()` call anchors the primary and its callers label,
/// entry → read.
#[test]
fn e78_an_uncovered_injected_call_traces_its_chain() {
    let source = r#"
import std::io::print;
import std::context::Context;

let current: Context<i32> = Context::new();

fun call_it(body: (|| void) context current) {
    body();
}

fun main() {
    call_it(|| print(current.get()));
}
main();
        "#;
    assert_fails_spanning(
        source,
        "body()",
        "an injected closure is called here, but this code can be reached without an enclosing `run` for context `current`",
    );
    assert_traces(
        source,
        "an injected closure is called here",
        &[
            (
                "main()",
                1,
                "the context requirement flows through this call",
            ),
            (
                "call_it(|| print(current.get()))",
                0,
                "the context requirement flows through this call",
            ),
        ],
    );
}

// --- B229: a `run` the solver never selected is still a `run` ---
//
// The context pass finds its sites by scanning `function_calls`, which holds
// only SELECTED calls. One `run` argument that fails to type leaves the
// method unselected, so the site vanishes from the scan, the context looks
// bound nowhere, and every strict read of it fences — a wall of refusals about
// a missing `run` the program plainly writes, with the one real error last.
// The fix reads the unresolved sites' shape and stands the coverage verdict
// down for exactly the contexts they name.

/// The owner's shape (kolt, 2026-09-04): a field added to the context struct
/// and not to the initializer. ONE error, at the initializer.
#[test]
fn b229_a_missing_initializer_field_does_not_fence_every_read() {
    let source = r#"
import std::io::print;
import std::context::Context;

struct AppCtx {
    theme: str,
    density: i32,
}

let app_ctx: Context<AppCtx> = Context::new();

fun label(): str {
    app_ctx.get().theme
}

fun badge(): str {
    app_ctx.get().theme + "-badge"
}

fun footer(): str {
    app_ctx.get().theme + "-footer"
}

fun component(): str {
    label() + badge() + footer()
}

fun main() {
    app_ctx.run(AppCtx { theme = "dark" }, || {
        print(component());
    });
}
main();
        "#;
    assert_fails_once_with(source, "`density` is missing");
    assert_fails_without(source, "is read here, but this code can be reached without");
}

/// The arity mismatch's other half: an EXTRA field cascades the same way, and
/// is stood down the same way.
#[test]
fn b229_an_extra_initializer_field_does_not_fence_every_read() {
    let source = r#"
import std::io::print;
import std::context::Context;

struct AppCtx {
    theme: str,
}

let app_ctx: Context<AppCtx> = Context::new();

fun label(): str {
    app_ctx.get().theme
}

fun main() {
    app_ctx.run(AppCtx { theme = "dark", density = 2 }, || {
        print(label());
    });
}
main();
        "#;
    assert_fails_once_with(source, "`density` is not a field of `AppCtx`");
    assert_fails_without(source, "is read here, but this code can be reached without");
}

/// The general form: ANY unresolved value argument deletes the site, so the
/// stand-down is keyed on the unselected call and not on the initializer.
#[test]
fn b229_an_unresolved_run_argument_does_not_fence_every_read() {
    let source = r#"
import std::io::print;
import std::context::Context;

struct AppCtx {
    theme: str,
}

let app_ctx: Context<AppCtx> = Context::new();

fun label(): str {
    app_ctx.get().theme
}

fun main() {
    app_ctx.run(missing_fn(), || {
        print(label());
    });
}
main();
        "#;
    assert_fails_once_with(source, "cannot find 'missing_fn' in this scope");
    assert_fails_without(source, "is read here, but this code can be reached without");
}

/// The invariant the stand-down may not weaken: an uncovered read in a program
/// with nothing else wrong is still a compile error, not a silent miscompile.
#[test]
fn b229_an_uncovered_read_in_a_clean_program_still_fences() {
    assert_fails_once_with(
        r#"
import std::io::print;
import std::context::Context;

struct AppCtx {
    theme: str,
}

let app_ctx: Context<AppCtx> = Context::new();

fun label(): str {
    app_ctx.get().theme
}

fun main() {
    print(label());
    app_ctx.run(AppCtx { theme = "dark" }, || {
        print(label());
    });
}
main();
        "#,
        "context `app_ctx` is read here, but this code can be reached without an enclosing `run`",
    );
}

/// And it is per CONTEXT, not per program: the context whose `run` failed
/// stands down; a second context's genuinely uncovered read in the same
/// program keeps its refusal.
#[test]
fn b229_only_the_unresolved_run_s_own_context_stands_down() {
    let source = r#"
import std::io::print;
import std::context::Context;

struct AppCtx {
    theme: str,
    density: i32,
}

struct Other {
    tint: str,
}

let app_ctx: Context<AppCtx> = Context::new();
let other_ctx: Context<Other> = Context::new();

fun label(): str {
    app_ctx.get().theme
}

fun tint(): str {
    other_ctx.get().tint
}

fun main() {
    print(tint());
    app_ctx.run(AppCtx { theme = "dark" }, || {
        print(label());
    });
}
main();
        "#;
    assert_fails_once_with(
        source,
        "context `other_ctx` is read here, but this code can be reached without an enclosing `run`",
    );
    assert_fails_without(source, "context `app_ctx` is read here");
}

/// B229's second face: a trace label on a METHOD-call hop points at the
/// callee's name, not at the whole receiver chain — which in a
/// component-shaped body is the body. The plain hops in the same trace
/// (`label()`, `panel(..)`, `main()`) are the control: they carry no member
/// name and keep their own already-tight spans.
#[test]
fn b229_a_method_call_hop_spans_the_callee_name() {
    let source = r#"
import std::io::print;
import std::context::Context;

let current: Context<i32> = Context::new();

fun label(): i32 {
    current.get()
}

struct Row {
    id: i32,
}

impl Row {
    fun render_body(self): i32 {
        label()
    }
}

fun panel(row: Row): i32 {
    row
        .render_body()
}

fun main() {
    print(panel(Row { id = 1 }));
}
main();
        "#;
    assert_traces(
        source,
        "context `current` is read here",
        &[
            (
                "main()",
                1,
                "the context requirement flows through this call",
            ),
            (
                "panel(Row { id = 1 })",
                0,
                "the context requirement flows through this call",
            ),
            (
                "render_body",
                1,
                "the context requirement flows through this call",
            ),
            (
                "label()",
                1,
                "the context requirement flows through this call",
            ),
        ],
    );
}

// --- E84: the demotion/trace contract widens to any dependency package ---
// (diagnostics-standard.md C3a, the owner's 2026-08-22 ruling): code the
// user did not write — std or ANY external/linked package — demotes and
// traces the same way. The seam is the loader's `Origin::Dep` (the
// `Workspace`/`PackageSpec` layer — the same classification the manifest
// resolver feeds), surfaced as `Program::dependency_sources`; never a path
// heuristic. Before the widening, a read inside a dependency anchored IN the
// dependency's file and the package's internal frames were labeled as hops.

/// A dependency-package fixture: its import name and its files (`lib.vl` at
/// the package root), staged on disk and handed to `analyze_source` as a
/// real `Workspace` package — the loader classifies it `Origin::Dep`
/// exactly as it does one resolved from a manifest.
struct DependencyFixture {
    import_name: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// Whether the package is a declared workspace MEMBER (`[project]
    /// packages`, E90) — the manifest resolver's classification, staged
    /// directly here since the harness enters the `Workspace` by hand
    /// (`manifest.rs` pins the resolver's own decision). A member is the
    /// user's code: never demoted. An external package (`false`) keeps the
    /// E84 demotion.
    member: bool,
}

/// One diagnostic from a workspace-with-dependencies analysis, fully
/// file-attributed so a pin can say WHERE each part landed: the primary's
/// file and exact spanned text, the C3 note's (message, file, spanned text),
/// and each trace hop's (label, file, spanned text, `call`), in analyzer
/// order.
struct WorkspaceDiagnostic {
    message: String,
    file: String,
    anchor: String,
    note: Option<(String, String, String)>,
    trace: Vec<(String, String, String, bool)>,
}

/// Analyzes `entry_files` (under an `app/` root; `main.vl` is the entry)
/// against `dependencies`, each staged in its own directory beside `app/`
/// and entered into the `Workspace` by hand — the same staging
/// `module_resolution.rs` uses, with the diagnostics kept whole (note and
/// trace included) instead of flattened to messages.
fn analyze_workspace_with_dependencies(
    entry_files: &[(&str, &str)],
    dependencies: &[DependencyFixture],
) -> Vec<WorkspaceDiagnostic> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_e84_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).unwrap();
    // Every file's text by its NAME, for slicing spans back into words.
    // Fixture file names are unique across packages by construction.
    let mut texts: Vec<(String, String)> = Vec::new();
    for (relative, contents) in entry_files {
        let path = app_dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        texts.push((
            Path::new(relative)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            contents.to_string(),
        ));
    }
    let mut packages = Vec::new();
    let mut entry_dependencies = Vec::new();
    for (index, dependency) in dependencies.iter().enumerate() {
        let dependency_root = root.join(dependency.import_name);
        for (relative, contents) in dependency.files {
            let path = dependency_root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, contents).unwrap();
            texts.push((
                Path::new(relative)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                contents.to_string(),
            ));
        }
        packages.push(PackageSpec {
            base_root: dependency_root,
            layers: Vec::new(),
            dependencies: Vec::new(),
            surface: true,
            member: dependency.member,
            prelude: Default::default(),
        });
        entry_dependencies.push((dependency.import_name.to_string(), index));
    }
    let workspace = Workspace {
        packages,
        entry_dependencies,
        ..Workspace::default()
    };

    let entry_path = app_dir.join("main.vl");
    let source = std::fs::read_to_string(&entry_path).unwrap();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let leaked: &'static str = Box::leak(source.into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    &app_dir,
                    &entry_path,
                    Some(Platform::default()),
                    &workspace,
                );
                // The entry's own parse errors lead; `diagnostic_sources` is
                // parallel to the program's half (as in `analyze_package`).
                let prefix = errors.len()
                    - program
                        .as_ref()
                        .map(|program| program.diagnostics.len())
                        .unwrap_or(0);
                let file_name_of = |source: vilan_core::analyzer::SourceId| -> Option<String> {
                    let program = program.as_ref()?;
                    let path = program.source_path(source)?;
                    Some(path.file_name()?.to_string_lossy().into_owned())
                };
                let text_at = |file: &Option<String>, span: vilan_core::span::Span| -> String {
                    file.as_deref()
                        .and_then(|file| {
                            texts
                                .iter()
                                .find(|(name, _)| name == file)
                                .map(|(_, text)| {
                                    text.get(span.into_range()).unwrap_or("").to_string()
                                })
                        })
                        .unwrap_or_default()
                };
                let diagnostics = errors
                    .iter()
                    .enumerate()
                    .map(|(index, error)| {
                        let primary_source = index
                            .checked_sub(prefix)
                            .and_then(|offset| {
                                let program = program.as_ref()?;
                                program.diagnostic_sources.get(offset).copied()
                            })
                            .unwrap_or(vilan_core::analyzer::SourceId(0));
                        let primary_file = file_name_of(primary_source);
                        // `Note::source` contract: `None` = the primary's file.
                        let located =
                            |source: Option<vilan_core::analyzer::SourceId>| -> Option<String> {
                                match source {
                                    Some(source) => file_name_of(source),
                                    None => primary_file.clone(),
                                }
                            };
                        WorkspaceDiagnostic {
                            message: error.msg.clone(),
                            anchor: text_at(&primary_file, error.span),
                            note: error.note.as_ref().map(|note| {
                                let file = located(note.source);
                                (
                                    note.msg.clone(),
                                    file.clone().unwrap_or_default(),
                                    text_at(&file, note.span),
                                )
                            }),
                            trace: error
                                .trace
                                .iter()
                                .map(|hop| {
                                    let file = located(hop.note.source);
                                    (
                                        hop.note.msg.clone(),
                                        file.clone().unwrap_or_default(),
                                        text_at(&file, hop.note.span),
                                        hop.call,
                                    )
                                })
                                .collect(),
                            file: primary_file.unwrap_or_default(),
                        }
                    })
                    .collect();
                let _ = std::fs::remove_dir_all(&root);
                diagnostics
            }))
            .unwrap_or_else(|_| panic!("the compiler panicked analyzing the E84 workspace fixture"))
        })
        .expect("spawn worker")
        .join()
        .expect("worker thread aborted")
}

/// The dependency counterpart of `e74_an_uncovered_effect_anchors_at_the_users_call`
/// (shape: the strict read sits directly in the dependency function the user
/// calls). Pre-widening (the probe, 2026-08-24): the primary anchored at
/// `current.get()` IN `lib.vl`, the user's calls rode as mere hops, and no
/// C3 note existed.
#[test]
fn e84_a_dependency_read_anchors_at_the_users_call() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[(
            "main.vl",
            "import std::io::print;\nimport depctx::read_it;\n\nfun main() {\n\tprint(read_it());\n}\nmain();\n",
        )],
        &[DependencyFixture {
            import_name: "depctx",
            files: &[(
                "lib.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun read_it(): i32 {\n\tcurrent.get()\n}\n",
            )],
            member: false,
        }],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert!(
        diagnostic.message.contains(
            "context `current` is read here, but this code can be reached without an enclosing `run`"
        ),
        "{message}",
        message = diagnostic.message
    );
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("main.vl", "read_it()"),
        "the primary anchors at the USER's call, never inside the package"
    );
    assert_eq!(
        diagnostic.note.as_ref().map(|(msg, file, text)| (
            msg.as_str(),
            file.as_str(),
            text.as_str()
        )),
        Some((
            "the read is inside `read_it` here",
            "lib.vl",
            "current.get()"
        )),
        "the C3 note demotes the package-internal read, in ITS file"
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![(
            "the context requirement flows through this call",
            "main.vl",
            "main()",
            true
        )],
        "the chain holds the user's calls only"
    );
}

/// The chain shape: the user calls the package's entry function and the read
/// sits two package-internal frames deeper. The exact-length trace assertion
/// is the internal-frames pin — pre-widening, `deep_read()` and `middle()`
/// (both inside `lib.vl`) were labeled as hops.
#[test]
fn e84_a_dependency_chains_hops_exclude_package_internals() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[(
            "main.vl",
            "import std::io::print;\nimport depctx::entry;\n\nfun main() {\n\tprint(entry());\n}\nmain();\n",
        )],
        &[DependencyFixture {
            import_name: "depctx",
            files: &[(
                "lib.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun deep_read(): i32 {\n\tcurrent.get()\n}\n\nfun middle(): i32 {\n\tdeep_read()\n}\n\nfun entry(): i32 {\n\tmiddle()\n}\n",
            )],
            member: false,
        }],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("main.vl", "entry()"),
        "the primary anchors at the user's call into the package"
    );
    assert_eq!(
        diagnostic.note.as_ref().map(|(msg, file, text)| (
            msg.as_str(),
            file.as_str(),
            text.as_str()
        )),
        Some((
            "the read is inside `deep_read` here",
            "lib.vl",
            "current.get()"
        )),
        "the note names the function holding the read, in the package's file"
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![(
            "the context requirement flows through this call",
            "main.vl",
            "main()",
            true
        )],
        "package-internal frames (`middle`, `deep_read`) are traversed but never labeled"
    );
}

/// The injected-call flavor (row 223) gains its first library-internal
/// incidence: the dependency declares the context AND the `context`-clause
/// function; the user's call site is the entry. Pre-widening the primary
/// anchored at `body()` inside `lib.vl`, note-free.
#[test]
fn e84_a_dependency_injected_call_demotes_the_same_way() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[(
            "main.vl",
            "import std::io::print;\nimport depctx::call_it;\n\nfun main() {\n\tcall_it(|| print(1));\n}\nmain();\n",
        )],
        &[DependencyFixture {
            import_name: "depctx",
            files: &[(
                "lib.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun call_it(body: (|| void) context current) {\n\tbody();\n}\n",
            )],
            member: false,
        }],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert!(
        diagnostic.message.contains(
            "an injected closure is called here, but this code can be reached without an enclosing `run` for context `current`"
        ),
        "{message}",
        message = diagnostic.message
    );
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("main.vl", "call_it(|| print(1))"),
        "the primary anchors at the user's call"
    );
    assert_eq!(
        diagnostic.note.as_ref().map(|(msg, file, text)| (
            msg.as_str(),
            file.as_str(),
            text.as_str()
        )),
        Some((
            "the injected call is inside `call_it` here",
            "lib.vl",
            "body()"
        )),
        "the C3 note demotes the package-internal injected call"
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![(
            "the context requirement flows through this call",
            "main.vl",
            "main()",
            true
        )],
        "the injected flavor traces through the same helpers as the read flavor"
    );
}

/// The C3a boundary, from the other side: a WORKSPACE sibling module (the
/// user's own `pkg::` code, `Origin::Pkg`) never demotes — the primary stays
/// at the read in the module's file, note-free, with the user-side chain as
/// the trace. This is what pins "non-workspace" to the loader's
/// classification rather than to "any other file".
#[test]
fn e84_a_workspace_sibling_read_still_anchors_at_itself() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::helper::read_it;\n\nfun main() {\n\tprint(read_it());\n}\nmain();\n",
            ),
            (
                "helper.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun read_it(): i32 {\n\tcurrent.get()\n}\n",
            ),
        ],
        &[],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("helper.vl", "current.get()"),
        "the user's own module anchors at the read (E74's no-over-correction half)"
    );
    assert!(
        diagnostic.note.is_none(),
        "no demotion note for code the user wrote: {note:?}",
        note = diagnostic.note
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![
            (
                "the context requirement flows through this call",
                "main.vl",
                "main()",
                true
            ),
            (
                "the context requirement flows through this call",
                "main.vl",
                "read_it()",
                true
            ),
        ],
        "the chain labels the user's calls, entry → read"
    );
}

// --- E90: workspace MEMBERS are carved out of the E84 demotion ---
// (diagnostics-standard.md C3a ruling note, RULED 2026-08-24): a `[project]`
// member reached through a path edge classifies `Origin::Dep` like any
// dependency, but it is code the user edits — so it gets full user
// treatment: reads anchor at themselves in the member's file, note-free,
// with member-internal calls labeled as hops. Only genuinely external
// packages (git, unlisted path) demote. Membership is the root manifest's
// `packages` declaration (`PackageSpec::member`, pinned in `manifest.rs`),
// never a path test; the harness stages the resolver's decision directly.

/// The member counterpart of `e84_a_dependency_chains_hops_exclude_package_internals`:
/// the same chain shape (entry → middle → deep_read → the read), now in a
/// declared workspace member. Pre-carve-out (the E84 tree) the primary
/// anchored at the user's `entry()` call with the read demoted to the C3
/// note and the member's internal frames unlabeled.
#[test]
fn e90_a_member_package_read_anchors_at_itself_with_its_chain_labeled() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[(
            "main.vl",
            "import std::io::print;\nimport common::entry;\n\nfun main() {\n\tprint(entry());\n}\nmain();\n",
        )],
        &[DependencyFixture {
            import_name: "common",
            files: &[(
                "lib.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun deep_read(): i32 {\n\tcurrent.get()\n}\n\nfun middle(): i32 {\n\tdeep_read()\n}\n\nfun entry(): i32 {\n\tmiddle()\n}\n",
            )],
            member: true,
        }],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("lib.vl", "current.get()"),
        "a member's read anchors AT ITSELF, in the member's file"
    );
    assert!(
        diagnostic.note.is_none(),
        "no demotion note for the user's own workspace member: {note:?}",
        note = diagnostic.note
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![
            (
                "the context requirement flows through this call",
                "main.vl",
                "main()",
                true
            ),
            (
                "the context requirement flows through this call",
                "main.vl",
                "entry()",
                true
            ),
            (
                "the context requirement flows through this call",
                "lib.vl",
                "middle()",
                true
            ),
            (
                "the context requirement flows through this call",
                "lib.vl",
                "deep_read()",
                true
            ),
        ],
        "member-internal calls are labeled like the user's own, entry → read"
    );
}

/// The same carve-out through the OTHER load site (the module loop, not the
/// `lib.vl` surface): the read sits in a member's module reached as
/// `common::util::read_it`. Pre-carve-out it demoted exactly like
/// `e90_an_external_packages_module_read_still_demotes`' shape.
#[test]
fn e90_a_member_packages_module_read_anchors_at_itself() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[(
            "main.vl",
            "import std::io::print;\nimport common::util::read_it;\n\nfun main() {\n\tprint(read_it());\n}\nmain();\n",
        )],
        &[DependencyFixture {
            import_name: "common",
            files: &[
                ("lib.vl", ""),
                (
                    "util.vl",
                    "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun read_it(): i32 {\n\tcurrent.get()\n}\n",
                ),
            ],
            member: true,
        }],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("util.vl", "current.get()"),
        "a member module's read anchors at itself"
    );
    assert!(
        diagnostic.note.is_none(),
        "no demotion note for a member's module: {note:?}",
        note = diagnostic.note
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![
            (
                "the context requirement flows through this call",
                "main.vl",
                "main()",
                true
            ),
            (
                "the context requirement flows through this call",
                "main.vl",
                "read_it()",
                true
            ),
        ],
        "the chain labels the calls, entry → read"
    );
}

/// The injected-call flavor (row 223) in a MEMBER: the member declares both
/// the context and the `context`-clause function, and carves out exactly
/// like the read flavor — the primary anchors at the member-internal
/// `body()` in the member's file, note-free, with the user's calls as the
/// chain. Pre-carve-out this shape demoted like
/// `e84_a_dependency_injected_call_demotes_the_same_way`.
#[test]
fn e90_a_member_packages_injected_call_anchors_at_itself() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[(
            "main.vl",
            "import std::io::print;\nimport common::call_it;\n\nfun main() {\n\tcall_it(|| print(1));\n}\nmain();\n",
        )],
        &[DependencyFixture {
            import_name: "common",
            files: &[(
                "lib.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun call_it(body: (|| void) context current) {\n\tbody();\n}\n",
            )],
            member: true,
        }],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert!(
        diagnostic.message.contains(
            "an injected closure is called here, but this code can be reached without an enclosing `run` for context `current`"
        ),
        "{message}",
        message = diagnostic.message
    );
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("lib.vl", "body()"),
        "a member's injected call anchors at itself, in the member's file"
    );
    assert!(
        diagnostic.note.is_none(),
        "no demotion note for the user's own workspace member: {note:?}",
        note = diagnostic.note
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![
            (
                "the context requirement flows through this call",
                "main.vl",
                "main()",
                true
            ),
            (
                "the context requirement flows through this call",
                "main.vl",
                "call_it(|| print(1))",
                true
            ),
        ],
        "the chain labels the user's calls, entry → injected call"
    );
}

/// The boundary's other side, at the module load site: an EXTERNAL package's
/// module (member: false — a git checkout or an unlisted path dep) keeps the
/// E84 demotion. This is the control that pins the carve-out to MEMBERSHIP —
/// an over-carve of every `Origin::Dep` package goes red here.
#[test]
fn e90_an_external_packages_module_read_still_demotes() {
    let diagnostics = analyze_workspace_with_dependencies(
        &[(
            "main.vl",
            "import std::io::print;\nimport depctx::util::read_it;\n\nfun main() {\n\tprint(read_it());\n}\nmain();\n",
        )],
        &[DependencyFixture {
            import_name: "depctx",
            files: &[
                ("lib.vl", ""),
                (
                    "util.vl",
                    "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun read_it(): i32 {\n\tcurrent.get()\n}\n",
                ),
            ],
            member: false,
        }],
    );
    assert_eq!(diagnostics.len(), 1, "one refusal, one primary");
    let diagnostic = &diagnostics[0];
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.anchor.as_str()),
        ("main.vl", "read_it()"),
        "an external package's module still demotes to the user's call"
    );
    assert_eq!(
        diagnostic.note.as_ref().map(|(msg, file, text)| (
            msg.as_str(),
            file.as_str(),
            text.as_str()
        )),
        Some((
            "the read is inside `read_it` here",
            "util.vl",
            "current.get()"
        )),
        "the C3 note demotes the read into the module's own file"
    );
    assert_eq!(
        diagnostic
            .trace
            .iter()
            .map(|(msg, file, text, call)| (msg.as_str(), file.as_str(), text.as_str(), *call))
            .collect::<Vec<_>>(),
        vec![(
            "the context requirement flows through this call",
            "main.vl",
            "main()",
            true
        )],
        "the chain holds the user's calls only"
    );
}

// The dead-reader exemption: a program that imports `std::reactive` without
// ever using the ambient layer must compile — an uncalled reader cannot run,
// so it cannot run uncovered.
#[test]
fn importing_reactive_without_the_ambient_layer_compiles() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Subscription, Disposable };

        fun main() {
            let count = Signal::new(5);
            let seen = count.sub(|value| print(value));
            seen.dispose();
        }

        main();
        "#,
        "5\n",
    );
}

// A DEAD user helper reaching the ambient reader must not poison the
// covered path beside it.
#[test]
fn a_dead_ambient_reader_does_not_poison_covered_paths() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope, get_owner };

        // Never called: exempt, and it must not unbind `get_owner` for the
        // covered path below.
        fun forgotten() {
            let owner = get_owner();
            owner.dispose();
        }

        fun main() {
            let count = Signal::new(7);
            let owner = Owner::new();
            owner_scope.run(owner, || {
                count.effect(|value| print(value));
            });
            print("alive");
        }

        main();
        "#,
        "7\nalive\n",
    );
}

// FIXED (backlog B14): the context pass now adds trait-dispatch edges
// locally — a default body reading a context is covered when its dispatch
// sites are, and the hidden value threads through the dispatch call.
#[test]
fn a_trait_default_body_reads_context_through_covered_dispatch() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let current: Context<i32> = Context::new();

        trait Probe {
            fun name(self): str;

            fun report(self) {
                print(i"{self.name()}: {current.get()}");
            }
        }

        struct Widget { tag: str }

        impl Widget with Probe {
            fun name(self): str {
                self.tag
            }
        }

        fun main() {
            current.run(9, || {
                Widget { tag = "w" }.report();
            });
        }

        main();
        "#,
        "w: 9\n",
    );
}

// FIXED with B14's slice: an inherited trait default called on a GENERIC
// subject's concrete instance (`SignalCell<i32>` inheriting from
// `impl SignalCell<type T> with Source<T>`) — `resolve_inherited_default`
// matched impl subjects by exact type equality, so generic subjects never
// matched and the call silently bound to the trait's ABSTRACT member (the
// B12 silent-miscompile shape). Now nominal, like `resolve_member_on_type`.
//
// MIGRATED for B174, and this is the whole of the estate's migration: the
// census swept std, the corpus, docs fences, examples, benchmarks, templates,
// kolt and the website and found exactly one trait default written over the
// trait's own UNBOUNDED parameter, which is this one. `T: Add` is orthogonal to
// what the fixture asserts — that an inherited default dispatches on a generic
// impl subject — and the answer is unchanged at `42`. It was an answer this
// declaration got by luck: `Holder { value = "ab" }.twice()` printed `abab`
// through the same default, one type argument from demonstrating the bug the
// fixture was silently relying on.
//
// The impl's own binder deliberately does NOT restate the bound. It does not
// have to: satisfaction is checked where the parameter is GROUNDED, so
// `Holder { value = Point { … } }.twice()` is refused at that call — "'Point'
// does not implement trait 'Add'", labelled at the trait's declaration —
// whether the binder repeats `: Add` or not. Restating it would make the
// migration look like two edits when it is one.
#[test]
fn an_inherited_default_on_a_generic_subject_dispatches() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Doubler<T: Add> {
            fun once(self): T;

            fun twice(self): T {
                self.once() + self.once()
            }
        }

        struct Holder<T> {
            value: T,
        }

        impl Holder<type T> with Doubler<T> {
            fun once(self): T {
                self.value
            }
        }

        fun main() {
            print(Holder { value = 21 }.twice());
        }

        main();
        "#,
        "42\n",
    );
}

// --- Context-typed closure parameters (proposal/ambient-owner.md §5, B15) ---

// The flagship: an injected closure rides a PLAIN function into `run` — the
// literal is born outside the extent and defers its binding to the call.
#[test]
fn an_injected_closure_rides_a_plain_wrapper_into_run() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let current: Context<i32> = Context::new();

        fun run_with(value: i32, body: (|| void) context current) {
            current.run(value, body);
        }

        fun main() {
            run_with(5, || print(current.get()));
            run_with(9, || print(current.get() + 1));
        }

        main();
        "#,
        "5\n10\n",
    );
}

// Injected values forward to parameters with the SAME clause, and calls
// through them thread the deferred argument on.
#[test]
fn injected_closures_forward_and_thread_through_calls() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let current: Context<i32> = Context::new();

        fun call_it(body: (|| void) context current) {
            body();
        }

        fun forward(body: (|| void) context current) {
            call_it(body);
        }

        fun main() {
            current.run(7, || {
                forward(|| print(current.get() + 100));
            });
        }

        main();
        "#,
        "107\n",
    );
}

// A multi-context clause: both deferred arguments supply, in clause order.
#[test]
fn a_multi_context_clause_injects_both_values() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let left: Context<i32> = Context::new();
        let right: Context<i32> = Context::new();

        fun call_it(body: (|| void) context (left, right)) {
            body();
        }

        fun main() {
            left.run(3, || {
                right.run(4, || {
                    call_it(|| print(left.get() * 10 + right.get()));
                });
            });
        }

        main();
        "#,
        "34\n",
    );
}

// Calling an injected closure is a read: an uncovered caller is fenced.
#[test]
fn an_uncovered_injected_call_is_a_compile_error() {
    let diagnostics = failure_diagnostics(
        r#"
import std::io::print;
import std::context::Context;

let current: Context<i32> = Context::new();

fun call_it(body: (|| void) context current) {
    body();
}

fun main() {
    call_it(|| print(current.get()));
}
main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("injected closure is called here")),
        "expected the injected-call fence; got: {diagnostics:#?}"
    );
}

// The value-flow restriction: an injected closure may be called, forwarded to
// a matching clause, or handed to `run` — nothing else.
#[test]
fn an_injected_closure_cannot_escape() {
    let source = r#"
import std::context::Context;

let current: Context<i32> = Context::new();

fun hold(body: (|| void) context current) {
    let escaped = body;
}

fun main() {}
main();
        "#;
    let diagnostics = failure_diagnostics(source);
    // The error anchors at the escaping USE (the second `body`), not the
    // parameter declaration.
    let use_site = source.rfind("body").unwrap();
    assert!(
        diagnostics.iter().any(|(message, range)| {
            message.contains("can only be called, forwarded") && range.start == use_site
        }),
        "expected the escape error at the use; got: {diagnostics:#?}"
    );
}

// Clause validation: the named value must be a context.
#[test]
fn a_clause_naming_a_non_context_errors() {
    let diagnostics = failure_diagnostics(
        r#"
import std::context::Context;

let unused: Context<i32> = Context::new();
let plain = 5;

fun bad(body: (|| void) context plain) {
    body();
}

fun main() {}
main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("names a value that is not a context")),
        "expected the non-context clause error; got: {diagnostics:#?}"
    );
}

// Receiver shape: `get` threads by binding identity, so a context that
// arrives as a call result has no name to thread by (ledger row 215).
#[test]
fn a_get_on_an_unnamed_context_receiver_is_rejected() {
    assert_fails_with(
        r#"
import std::context::Context;

let current: Context<i32> = Context::new();

fun pick(): Context<i32> {
    current
}

fun main() {
    let value = pick().get();
}
main();
        "#,
        "`get` must be called on a context bound to a name",
    );
}

// Receiver shape, `run`'s short arm (ledger row 216): no named context could
// be extracted at all. The message is a strict prefix of the closure-value
// arm's (row 217), so the pin also asserts the continuation is ABSENT —
// otherwise it could pass on the wrong arm.
#[test]
fn a_run_on_an_unnamed_context_receiver_is_rejected() {
    let diagnostics = failure_diagnostics(
        r#"
import std::context::Context;

let current: Context<i32> = Context::new();

fun pick(): Context<i32> {
    current
}

fun main() {
    pick().run(1, || {});
}
main();
        "#,
    );
    assert!(
        diagnostics.iter().any(|(message, _)| {
            message.contains("`run` must be called on a named context with a closure literal body")
                && !message.contains("or a closure value")
        }),
        "expected the short run-shape arm (no trailing closure-value alternative); got: {diagnostics:#?}"
    );
}

// A clause-typed `let` binding is a NAMED injected closure: its initializer
// must be a closure literal or a same-clause value — a call result is an
// escape the threading cannot follow (ledger row 219).
#[test]
fn a_context_typed_binding_with_a_non_literal_initializer_is_rejected() {
    assert_fails_with(
        r#"
import std::context::Context;

let current: Context<i32> = Context::new();

fun make(): || void {
    || {}
}

fun main() {
    let handler: (|| void) context current = make();
    handler();
}
main();
        "#,
        "a `context`-typed binding takes a closure literal, or a value with the same `context` clause",
    );
}

// A clause parameter's argument must be a closure literal, a same-clause
// value, or an adoptable local closure binding — anything else (here a call
// result) cannot receive the threaded context (ledger row 220).
#[test]
fn a_context_typed_parameter_with_a_non_closure_argument_is_rejected() {
    assert_fails_with(
        r#"
import std::context::Context;

let current: Context<i32> = Context::new();

fun make(): || void {
    || {}
}

fun call_it(body: (|| void) context current) {
    body();
}

fun main() {
    call_it(make());
}
main();
        "#,
        "a `context`-typed parameter takes a closure literal, a value with the same `context` clause, or a local closure binding (which adopts the clause)",
    );
}

// Clause placement: closure types only.
#[test]
fn a_clause_on_a_non_closure_type_errors() {
    let diagnostics = failure_diagnostics(
        r#"
import std::context::Context;

let current: Context<i32> = Context::new();

fun bad(value: (i32) context current) {}

fun main() {}
main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("only supported on a closure type")),
        "expected the placement error; got: {diagnostics:#?}"
    );
}

// Clause resolution: unknown names error at the name.
#[test]
fn a_clause_naming_an_unknown_value_errors() {
    assert_fails_spanning(
        r#"
fun bad(body: (|| void) context missing_name) {
    body();
}

fun main() {}
main();
        "#,
        "missing_name",
        "cannot find context `missing_name`",
    );
}

// Duplicate contexts in one clause error.
#[test]
fn a_duplicate_context_in_a_clause_errors() {
    let diagnostics = failure_diagnostics(
        r#"
import std::context::Context;

let current: Context<i32> = Context::new();

fun bad(body: (|| void) context (current, current)) {
    body();
}

fun main() {}
main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("duplicate context `current`")),
        "expected the duplicate error; got: {diagnostics:#?}"
    );
}

// `run` accepts an injected value only when its clause is exactly the run's
// context.
#[test]
fn run_rejects_a_mismatched_injected_body() {
    let diagnostics = failure_diagnostics(
        r#"
import std::context::Context;

let current: Context<i32> = Context::new();
let other: Context<i32> = Context::new();

fun mismatch(body: (|| void) context current) {
    other.run(1, body);
}

fun main() {}
main();
        "#,
    );
    assert!(
        diagnostics.iter().any(|(message, _)| {
            message.contains("closure value whose type is `context`-annotated")
        }),
        "expected the run-mismatch error; got: {diagnostics:#?}"
    );
}

// FIXED alongside B15: a context that is created but never read or run no
// longer emits a dangling `Context::new()` call — the news lower on the
// early path too.
#[test]
fn an_unused_context_compiles_and_runs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let current: Context<i32> = Context::new();

        fun main() {
            print("quiet");
        }

        main();
        "#,
        "quiet\n",
    );
}

// `Context.run` yields its body's value (the `batch` shape): direct,
// expression-position, and void bodies stay compatible.
#[test]
fn run_yields_the_body_value() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let current: Context<i32> = Context::new();

        fun main() {
            let answer = current.run(21, || current.get() * 2);
            print(answer);
            print(current.run(5, || current.get() + 1) + 100);
            current.run(1, || {
                print(current.get());
            });
        }

        main();
        "#,
        "42\n106\n1\n",
    );
}

// `comp` — the component scope: the body's product pairs with the disposal
// handle, and the component's effects die with it.
#[test]
fn comp_returns_the_product_and_the_scope() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, comp };

        fun main() {
            let count = Signal::new(1);
            let (label, scope) = comp(|| {
                count.effect(|value| print(value));
                "built"
            });
            print(label);
            count.set(2);
            scope.dispose();
            count.set(3);
            print("done");
        }

        main();
        "#,
        "1\nbuilt\n2\ndone\n",
    );
}

// `run_with_owner` yields its body's value too.
#[test]
fn run_with_owner_yields_the_body_value() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, run_with_owner };

        fun main() {
            let owner = Owner::new();
            print(run_with_owner(owner, || 40 + 2));
        }

        main();
        "#,
        "42\n",
    );
}

// The clause may name an IMPORTED context (the `std::ui` shape) — resolution
// runs after the import fixpoint, following the import alias to the defining
// binding so identity agrees with the threading pass.
#[test]
fn a_clause_can_name_an_imported_context() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope, run_with_owner };

        fun boundary(body: (|| void) context owner_scope) {
            let owner = Owner::new();
            run_with_owner(owner, || body());
        }

        fun main() {
            let count = Signal::new(4);
            boundary(|| count.effect(|value| print(value)));
            print("ok");
        }

        main();
        "#,
        "4\nok\n",
    );
}

// --- B217: a generated type miss resolved in `build()` anchors at the attribute
//
// B188's anchoring rule re-anchors a diagnostic raised against GENERATED code at
// the attribute that generated it — a template is not a file, and its spans
// index text no file holds. The redirect covered the walk's own diagnostics and
// the whole-program passes (`Program::anchored`), and missed the route in
// between: a written type annotation is not resolved during the walk at all, it
// is PREPPED and drained in `build()`, long after the generated walk closed. Two
// of B201's four pre-fix diagnostics came out that way and landed at the
// declaring `mod` and at the type, because `push_in_source` had a bare
// `SourceId` and no entity id to look an origin up by.
//
// The plant is a derive generator of the test's own: a macro that emits a member
// returning a type nobody declared.

const PLANTS_A_MISSING_TYPE: &str = r#"
        import std::io::print;

        macro fun Planted(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            source("impl " + target.name + " {\nfun planted(self): NoSuchType {\nself\n}\n}\n")
        }

        [derive(Planted)]
        struct Widget {
            size: i32,
        }

        fun main() { print(Widget { size = 3 }.size); }
        main();
        "#;

#[test]
fn b217_a_generated_type_miss_is_anchored_at_the_deriving_attribute() {
    // The message says the provenance, exactly as the walk's own route does.
    assert_fails_with(
        PLANTS_A_MISSING_TYPE,
        "in code generated by this attribute: cannot find type 'NoSuchType'",
    );
}

#[test]
fn b217_the_generated_type_miss_spans_the_derive_and_not_the_deriving_item() {
    // The half a message cannot state: WHERE the label is drawn. Pre-fix the
    // span indexed the template, which the deriving file does not hold, and the
    // label landed on whatever that file happened to have at those offsets.
    // The anchor is the derive's own name — the `[derive(Planted)]` occurrence,
    // not the `macro fun Planted` that generated it, which is the location
    // acting on the report means editing (standard A2).
    assert_fails_spanning_nth(
        PLANTS_A_MISSING_TYPE,
        "Planted",
        1,
        "cannot find type 'NoSuchType'",
    );
}

#[test]
fn b217_a_hand_written_type_miss_is_still_reported_where_it_was_written() {
    // The control: the redirect keys on the type id's own walk, so an
    // annotation the AUTHOR wrote keeps its own span and says nothing about an
    // attribute — in the same program that carries the generated one.
    assert_fails_spanning(
        r#"
        import std::io::print;

        macro fun Planted(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            source("impl " + target.name + " {\nfun planted(self): NoSuchType {\nself\n}\n}\n")
        }

        [derive(Planted)]
        struct Widget {
            size: i32,
        }

        fun by_hand(): AlsoMissing { 1 }

        fun main() { print(Widget { size = 3 }.size); }
        main();
        "#,
        "AlsoMissing",
        "cannot find type 'AlsoMissing'",
    );
}
