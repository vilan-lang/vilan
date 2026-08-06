//! The bindgen gate (E31, `proposal/bindgen.md` §6).
//!
//! Three layers, in the order a failure is easiest to read:
//!
//! 1. **Per-construct pins.** One test per row of §3's mapping table, asserting
//!    the specific thing that row promises. A broken mapping names itself
//!    instead of showing up as a golden diff.
//! 2. **Golden fixtures** (`tests/bindgen/*.d.ts` paired with `*.vl`), compared
//!    BYTE-FOR-BYTE, the discipline the corpus gate already uses. A change to
//!    the emitter that alters a golden is either a bug or a deliberate,
//!    reviewed improvement — never silently regenerated.
//! 3. **The goldens compile**, through the real analyzer, and are **byte-stable
//!    across runs** — the constraint §3.8's synthesized-name heuristic and the
//!    collision renamer both have to satisfy.
//!
//! Plus the two owner-note pins, which fix LANGUAGE facts bindgen's design
//! rests on. Both are written so that the language changing under bindgen turns
//! them red rather than leaving a stale design in place.

use std::path::{Path, PathBuf};

use vilan_core::bindgen::{Options, generate};
use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

fn fixtures_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/bindgen")
}

fn options() -> Options {
    Options {
        platform: "node".to_string(),
        source_name: "fixture.d.ts".to_string(),
    }
}

/// Generates bindings for an inline `.d.ts` snippet.
fn bind(source: &str) -> String {
    generate(source, &options()).source
}

fn bind_for(platform: &str, source: &str) -> String {
    generate(
        source,
        &Options {
            platform: platform.to_string(),
            source_name: "fixture.d.ts".to_string(),
        },
    )
    .source
}

/// Asserts `needle` appears in the generated output, printing the whole output
/// on failure — a mapping bug is unreadable without it.
#[track_caller]
fn assert_emits(source: &str, needle: &str) {
    let output = bind(source);
    assert!(
        output.contains(needle),
        "expected the bindings to contain:\n  {needle}\n\nbut got:\n{output}"
    );
}

#[track_caller]
fn assert_does_not_emit(source: &str, needle: &str) {
    let output = bind(source);
    assert!(
        !output.contains(needle),
        "expected the bindings NOT to contain:\n  {needle}\n\nbut got:\n{output}"
    );
}

// --- §3.1 primitives and §3.6 functions --------------------------------------

#[test]
fn a_plain_function_binds_to_an_extern_call() {
    assert_emits(
        "declare function greet(name: string): string;",
        "[extern(\"greet\")]\n[platform(\"node\")]\nexternal fun greet(name: str): str;",
    );
}

#[test]
fn every_number_defaults_to_f64() {
    // §3.1: a `.d.ts` cannot say whether a `number` means an integer, so the
    // only lossless default is `f64`; narrowing is a human edit.
    assert_emits(
        "declare function at(index: number): number;",
        "at(index: f64): f64",
    );
}

#[test]
fn a_void_return_stays_void_and_a_never_return_is_diagnosed() {
    assert_emits(
        "declare function stop(): void;",
        "external fun stop(): void;",
    );
    let output = bind("declare function halt(): never;");
    assert!(output.contains("TODO(bindgen): TS `never`"), "{output}");
    assert!(output.contains("external fun halt(): void;"), "{output}");
}

#[test]
fn an_optional_parameter_becomes_one_binding_per_call_arity() {
    // §3.2 maps an optional parameter to `Option<T>`. CORRECTED at take-up:
    // `Option` cannot cross a host boundary at all (see
    // `option_cannot_cross_a_host_boundary_in_either_direction`). TS optionals
    // are trailing, so the exact mapping is one binding per real call arity —
    // both of the same host symbol, which is what std already does by hand
    // (`append`/`append_text` are two bindings of one `appendChild`).
    let output = bind("declare function log(message: string, level?: string): void;");
    assert!(
        output.contains("external fun log(message: str): void;"),
        "{output}"
    );
    assert!(
        output.contains("external fun log_with_level(message: str, level: str): void;"),
        "{output}"
    );
    // The prose explains why `Option` is absent; no TYPE is an `Option`.
    assert!(!output.contains(": Option<"), "{output}");
    // Both arities bind the SAME symbol.
    assert_eq!(output.matches("[extern(\"log\")]").count(), 2, "{output}");
}

#[test]
fn more_than_one_optional_parameter_binds_the_ends_and_names_the_middle() {
    let output = bind("declare function draw(a: string, b?: string, c?: string): void;");
    assert!(
        output.contains("external fun draw(a: str): void;"),
        "{output}"
    );
    assert!(
        output.contains("external fun draw_with_b_and_c(a: str, b: str, c: str): void;"),
        "{output}"
    );
    assert!(output.contains("has 2 optional parameters"), "{output}");
}

#[test]
fn a_promise_return_makes_the_binding_async() {
    assert_emits(
        "declare function load(url: string): Promise<string>;",
        "async external fun load(url: str): str;",
    );
}

#[test]
fn a_void_returning_callback_is_a_plain_closure_and_a_promise_one_is_async() {
    // §3.6: the divergence rule already lets an async vilan closure fill a
    // `|T| void` slot, so an event handler needs no annotation. A callback the
    // host AWAITS must be typed `async`, because adaptation never crosses a
    // host boundary.
    assert_emits(
        "declare function on(handler: (value: number) => void): void;",
        "external fun on(handler: |f64| void): void;",
    );
    assert_emits(
        "declare function commit(handler: (value: number) => Promise<string>): void;",
        "external fun commit(handler: async |f64| str): void;",
    );
}

#[test]
fn a_tuple_maps_across_exactly() {
    // The one aggregate row that IS representation-correct: a vilan tuple is a
    // JS array. `a_vilan_struct_is_a_positional_array` pins the fact underneath.
    assert_emits(
        "declare function span(): [string, number];",
        "external fun span(): (str, f64);",
    );
}

#[test]
fn a_rest_parameter_is_bound_as_one_list_and_diagnosed() {
    let output = bind("declare function join(...parts: string[]): string;");
    assert!(
        output.contains("external fun join(parts: List<str>): str;"),
        "{output}"
    );
    assert!(output.contains("TODO(bindgen): rest parameter"), "{output}");
    // The declared type is ALREADY the array; wrapping it again was a real bug.
    assert!(!output.contains("List<List<str>>"), "{output}");
}

// --- §3.7/§3.8 interfaces, classes, properties -------------------------------

#[test]
fn every_interface_becomes_an_external_struct_never_a_plain_struct() {
    // Not an ergonomics preference: `a_vilan_struct_is_a_positional_array`
    // shows a plain `struct` reads the wrong slots of a host object.
    let output = bind("interface Options { title: string; }");
    assert!(output.contains("external struct Options;"), "{output}");
    assert!(!output.contains("\nstruct Options"), "{output}");
}

#[test]
fn a_readonly_property_gets_a_getter_and_no_setter() {
    // §3.7: read-only-ness falls out of which externs bindgen writes; nothing
    // in the language needs to know about it.
    let output = bind("interface Options { readonly id: string; title: string; }");
    assert!(output.contains("[extern(get, \"id\")]"), "{output}");
    assert!(!output.contains("[extern(set, \"id\")]"), "{output}");
    assert!(output.contains("[extern(set, \"title\")]"), "{output}");
}

#[test]
fn a_get_set_accessor_pair_becomes_a_getter_and_a_setter() {
    let output = bind("interface Options { get name(): string; set name(value: string); }");
    assert!(output.contains("[extern(get, \"name\")]"), "{output}");
    assert!(output.contains("[extern(set, \"name\")]"), "{output}");
}

#[test]
fn an_interface_gets_an_object_constructor_for_the_options_bag_direction() {
    // The `RequestInit` precedent (§3.2): an options bag is a fresh `{}` filled
    // in by setters, which is also how the omitted-key question stays answerable.
    assert_emits(
        "interface Init { method: string; }",
        "[extern(\"Object\")]\n\texternal fun new(): Init;",
    );
}

#[test]
fn a_class_constructor_binds_to_extern_new_returning_the_class() {
    assert_emits(
        "declare class Widget { constructor(title: string); }",
        "[extern(new, \"Widget\")]\n\t[platform(\"node\")]\n\texternal fun new(title: str): Widget;",
    );
}

#[test]
fn a_generic_class_constructor_returns_the_applied_type() {
    assert_emits(
        "declare class Box<T> { constructor(value: T); }",
        "external fun new(value: T): Box<T>;",
    );
}

#[test]
fn a_static_member_binds_to_a_dotted_global() {
    // §3.7: a function without `self` is a static, reached as `Subject::name`.
    let output =
        bind("declare class Widget { static create(): Widget; static readonly tag: string; }");
    assert!(output.contains("[extern(\"Widget.create\")]"), "{output}");
    assert!(
        output.contains("external fun create(): Widget;"),
        "{output}"
    );
    assert!(output.contains("[extern(\"Widget.tag\")]"), "{output}");
    assert!(output.contains("external fun tag(): str;"), "{output}");
}

#[test]
fn an_anonymous_object_type_gets_a_synthesized_named_struct() {
    // §3.8.2: vilan has no anonymous struct types, so the name is derived from
    // the enclosing symbol and the member — never a traversal counter, or
    // `regenerating_the_same_input_is_byte_stable` would fail by construction.
    let output = bind("declare function serve(options: { port: number }): void;");
    assert!(output.contains("external struct ServeOptions;"), "{output}");
    assert!(
        output.contains("external fun serve(options: ServeOptions): void;"),
        "{output}"
    );
}

// --- §3.3 unions -------------------------------------------------------------

#[test]
fn an_absence_union_binds_the_bare_type_and_names_the_absence() {
    // §3.2 maps `T | null` to `Option<T>`. CORRECTED: an `Option` cannot be
    // READ from a host either — a present `"hello"` is matched as
    // `value[0] === 0`, i.e. `"h" === 0`, so it reads as `None`.
    for source in [
        "interface A { a: string | null; }",
        "interface A { a: string | undefined; }",
        "interface A { a: string | null | undefined; }",
    ] {
        let output = bind(source);
        assert!(output.contains("external fun a(self): str;"), "{output}");
        assert!(
            output.contains("may be `null`/`undefined` at the host"),
            "{output}"
        );
        assert!(!output.contains(": Option<"), "{output}");
    }
}

#[test]
fn nothing_bindgen_emits_ever_mentions_option() {
    // The rule this collapses to: `Option` is a vilan tagged array, so it never
    // appears in a binding to a THIRD-PARTY host. (std uses it across
    // `external` boundaries it owns — compiler intrinsics and its own `__`
    // runtime helpers, which know the representation.)
    for source in [
        "declare function f(a?: string): void;",
        "interface A { a?: string; }",
        "interface A { a: string | null; }",
        "declare function g(): string | undefined;",
    ] {
        assert_does_not_emit(source, ": Option<");
    }
}

#[test]
fn a_string_literal_union_alias_becomes_an_enum_with_a_match_wrapper() {
    // §3.3's highest-value union case, and the one place bindgen generates real
    // logic rather than a bare declaration.
    let output = bind(
        "type Align = \"start\" | \"end\";\ninterface Chart { setAlign(align: Align): void; }",
    );
    assert!(output.contains("enum Align {"), "{output}");
    assert!(output.contains("\tStart,"), "{output}");
    assert!(output.contains("\tEnd,"), "{output}");
    // The host boundary still speaks the raw string, so the extern takes `str`
    // and is hidden behind the wrapper.
    assert!(
        output.contains("external fun set_align_raw(self, align: str): void;"),
        "{output}"
    );
    assert!(output.contains("[doc(hidden)]"), "{output}");
    assert!(
        output.contains("fun set_align(self, align: Align): void {"),
        "{output}"
    );
    assert!(output.contains("Align::Start => \"start\","), "{output}");
    assert!(output.contains("Align::End => \"end\","), "{output}");
}

#[test]
fn an_inline_string_literal_union_widens_to_str_without_a_todo() {
    // Widening a CLOSED string set to `str` is total and safe — the host takes
    // exactly that string — so this is not a TODO. Only a named alias earns an
    // enum, because only then did a human name the concept.
    let output = bind("interface Chart { align(side: \"left\" | \"right\"): void; }");
    assert!(
        output.contains("external fun align(self, side: str): void;"),
        "{output}"
    );
    // The header explains what `TODO(bindgen)` means; no ACTUAL todo is emitted.
    assert!(!output.contains("TODO(bindgen):"), "{output}");
}

#[test]
fn an_open_union_widens_to_any_with_a_todo() {
    let output = bind("interface A { a: string | number; }");
    assert!(output.contains("external fun a(self): any;"), "{output}");
    assert!(
        output.contains("TODO(bindgen): TS union `string | number` widened to `any`"),
        "{output}"
    );
}

#[test]
fn a_discriminated_union_is_diagnosed_rather_than_mapped_to_an_enum() {
    // `proposal/bindgen.md` §3.3 recommends an `enum` here. VERIFIED wrong at a
    // host boundary: vilan's enum lowers to `[tag, …payload]` while the TS union
    // is a tagged OBJECT, so `match` reads `value[0]`, matches nothing, and
    // crashes. See `a_vilan_struct_is_a_positional_array` for the underlying
    // representation fact.
    let output = bind(
        "interface Chart { shape: { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number }; }",
    );
    assert!(
        output.contains("TODO(bindgen): discriminated union on `kind`"),
        "{output}"
    );
    assert!(
        output.contains("external fun shape(self): any;"),
        "{output}"
    );
}

#[test]
fn an_alias_of_an_unmappable_shape_declares_nothing_and_is_never_referenced() {
    // vilan has no type alias (§5), so an alias that maps to no declaration must
    // not leave its NAME usable — a member typed by it would reference a type
    // that was never written and the whole file would stop compiling.
    let output = bind("type Either = string | number;\ninterface Chart { value: Either; }");
    assert!(!output.contains(": Either"), "{output}");
    assert!(
        output.contains("external fun value(self): any;"),
        "{output}"
    );
}

// --- §3.9 index signatures (OWNER NOTE 2) ------------------------------------

#[test]
fn a_numeric_index_signature_is_never_mapped_to_list() {
    // OWNER NOTE 2, settled by running the compiler rather than reading the
    // proposal: `{ [index: number]: T }` describes an array-LIKE object, not a
    // JS array. `List<T>`'s whole iteration-based API (`for`-in, `map`,
    // `filter`, `fold`, `reverse`) throws `TypeError: … is not iterable` on
    // one, and a real array with HOLES hands `undefined` to a `T`-typed slot.
    let output = bind("interface Slots { [index: number]: string; }");
    assert!(
        output.contains("TODO(bindgen): numeric index signature"),
        "{output}"
    );
    assert!(!output.contains("List<str>"), "{output}");
}

#[test]
fn a_string_index_signature_is_never_mapped_to_map() {
    // Same root cause: `std::map::Map` is a plain vilan struct wrapping a
    // `NativeMap` keyed by `key.hash()`, not a host `{a: 1}` object.
    let output = bind("interface Lookup { [key: string]: number; }");
    assert!(
        output.contains("TODO(bindgen): string index signature"),
        "{output}"
    );
    // The TODO prose MENTIONS `Map<str, T>` to say why it is wrong; what must
    // not appear is `Map` in a type position.
    assert!(!output.contains(": Map<"), "{output}");
}

#[test]
fn a_mixed_named_and_index_interface_keeps_its_named_members() {
    // Better than the proposal's "no attempt at the hybrid": the named members
    // bind fine, and only the index signature is diagnosed.
    let output = bind("interface Mixed { length: number; [index: number]: string; }");
    assert!(
        output.contains("external fun length(self): f64;"),
        "{output}"
    );
    assert!(
        output.contains("TODO(bindgen): numeric index signature"),
        "{output}"
    );
}

#[test]
fn an_array_type_does_map_to_list() {
    // The row that IS true: a `T[]` in a `.d.ts` is a real JS array.
    assert_emits(
        "interface A { items: string[]; }",
        "external fun items(self): List<str>;",
    );
    assert_emits(
        "interface A { items: Array<string>; }",
        "external fun items(self): List<str>;",
    );
}

// --- §3.5 generics and inheritance -------------------------------------------

#[test]
fn generics_map_to_vilan_generics_with_the_impl_binder_form() {
    let output = bind("interface Box<T> { value: T; }");
    assert!(output.contains("external struct Box<T>;"), "{output}");
    assert!(output.contains("impl Box<type T> {"), "{output}");
    assert!(output.contains("external fun value(self): T;"), "{output}");
}

#[test]
fn extends_flattens_the_base_members_and_the_derived_member_wins() {
    // vilan has no struct inheritance, so a base's members are copied in —
    // what a human writing this binding by hand does. What it cannot recover is
    // ASSIGNABILITY, which stays a documented nominal-typing limit.
    let output = bind(
        "interface Node { id: string; remove(): void; }\n\
         interface Button extends Node { id: number; press(): void; }",
    );
    assert!(
        output.contains("external fun press(self): void;"),
        "{output}"
    );
    assert!(
        output.contains("external fun remove(self): void;"),
        "{output}"
    );
    // The derived `id: number` shadows the base's `id: string`.
    let button = output
        .split("external struct Button;")
        .nth(1)
        .unwrap_or_default();
    assert!(button.contains("external fun id(self): f64;"), "{button}");
    assert!(!button.contains("external fun id(self): str;"), "{button}");
}

#[test]
fn a_generic_base_substitutes_its_type_arguments_into_the_flattened_members() {
    let output =
        bind("interface Boxed<T> { value: T; }\ninterface StringBox extends Boxed<string> { }");
    let box_ = output
        .split("external struct StringBox;")
        .nth(1)
        .unwrap_or_default();
    assert!(box_.contains("external fun value(self): str;"), "{box_}");
}

#[test]
fn an_unresolved_base_is_diagnosed_rather_than_ignored() {
    assert_emits(
        "interface Orphan extends Elsewhere { own: string; }",
        "TODO(bindgen): `extends Elsewhere`",
    );
}

// --- §3.10 overloads ---------------------------------------------------------

#[test]
fn overloads_keep_the_first_signature_and_quote_the_rest() {
    let output = bind(
        "declare function parse(source: string): string;\n\
         declare function parse(source: number): string;",
    );
    assert!(
        output.contains("external fun parse(source: str): str;"),
        "{output}"
    );
    assert!(
        output.contains("1 additional overload(s) of `parse` not represented"),
        "{output}"
    );
    assert!(
        output.contains("declare function parse(source: number): string"),
        "{output}"
    );
    // One name, one signature: a second `external fun parse` would not compile.
    assert_eq!(output.matches("external fun parse(").count(), 1, "{output}");
}

#[test]
fn method_overloads_collapse_the_same_way() {
    let output = bind("interface P { draw(): void; draw(target: string): void; }");
    assert_eq!(output.matches("external fun draw(").count(), 1, "{output}");
    assert!(
        output.contains("1 additional overload(s) of `draw`"),
        "{output}"
    );
}

// --- §5 out-of-scope constructs must diagnose, not vanish --------------------

#[test]
fn a_namespace_is_diagnosed_rather_than_dropped() {
    assert_emits(
        "declare namespace Legacy { function helper(): void; }",
        "TODO(bindgen): namespace `Legacy`",
    );
}

#[test]
fn a_module_declaration_and_a_global_augmentation_are_diagnosed() {
    assert_emits(
        "declare module \"pkg\" { interface Extra { a: string } }",
        "TODO(bindgen): module declaration",
    );
    assert_emits(
        "declare global { interface Window { a: string } }",
        "TODO(bindgen): ambient global augmentation",
    );
}

#[test]
fn a_typescript_enum_is_diagnosed() {
    assert_emits(
        "declare enum Colour { Red, Green }",
        "TODO(bindgen): TypeScript enum `Colour`",
    );
}

#[test]
fn a_global_variable_is_diagnosed_because_no_extern_form_reads_one() {
    // Every `[extern(…)]` form binds a CALL or a receiver's property; none reads
    // a bare global as a value. This is the single biggest gap the lib.dom
    // probe found, so it is pinned here rather than left as folklore.
    assert_emits(
        "declare const document: Document;",
        "TODO(bindgen): `document` is a global VALUE",
    );
}

#[test]
fn conditional_and_mapped_types_are_diagnosed_at_the_point_of_use() {
    assert_emits(
        "interface A { a: string extends number ? boolean : string; }",
        "TODO(bindgen): conditional type",
    );
    // A mapped type wears an object type's braces but is a different construct;
    // reading it as one produced nonsense members before this was recognized.
    let output = bind("type M<T> = { [K in keyof T]: string };");
    assert!(output.contains("mapped type"), "{output}");
    assert!(!output.contains("external struct M"), "{output}");
}

#[test]
fn a_utility_type_and_an_intersection_are_diagnosed() {
    assert_emits("interface A { a: Partial<B>; }", "mapped utility type");
    assert_emits(
        "interface A { a: { x: string } & { y: number }; }",
        "TS intersection",
    );
}

#[test]
fn an_unresolved_type_reference_widens_to_any_with_a_todo() {
    // v1 does not resolve across files (§2), so a name this file never declares
    // cannot be emitted — it would not compile.
    let output = bind("interface A { a: SomeImportedThing; }");
    assert!(output.contains("is not declared in this file"), "{output}");
    assert!(output.contains("external fun a(self): any;"), "{output}");
}

// --- §4 platform attribution -------------------------------------------------

#[test]
fn every_emitted_extern_carries_the_platform_fence() {
    // §4: bindgen output lands in USER code, which is unconstrained absent a
    // fence — a generated browser-only binding in a node project would compile
    // clean and fail at runtime. `[platform(…)]` is function-only, so the fence
    // repeats on every binding; there is no struct-level form.
    let output = bind_for(
        "browser",
        "interface E { id: string; click(): void; }\ndeclare function q(s: string): E;",
    );
    let externs = output.matches("external fun").count();
    // Every extern but the interface's `Object` constructor is fenced.
    assert_eq!(
        output.matches("[platform(\"browser\")]").count(),
        externs - 1,
        "{output}"
    );
}

#[test]
fn the_platform_flag_is_checked_against_the_languages_own_vocabulary() {
    for accepted in ["node", "deno", "bun", "browser", "@process", "node:24"] {
        assert!(
            Options {
                platform: accepted.to_string(),
                source_name: "x.d.ts".to_string(),
            }
            .validate()
            .is_ok(),
            "{accepted} should be accepted"
        );
    }
    let rejected = Options {
        platform: "windows".to_string(),
        source_name: "x.d.ts".to_string(),
    };
    assert!(rejected.validate().is_err());
}

// --- Naming ------------------------------------------------------------------

#[test]
fn member_names_become_snake_case_matching_the_hand_written_std_dialect() {
    assert_emits(
        "interface D { getElementById(id: string): void; }",
        "fun get_element_by_id(",
    );
    assert_emits("interface D { innerHTML: string; }", "fun inner_html(");
    assert_emits("interface D { toJSON(): string; }", "fun to_json(");
    assert_emits(
        "interface D { XMLHttpRequest(): void; }",
        "fun xml_http_request(",
    );
}

#[test]
fn a_vilan_keyword_member_name_is_escaped() {
    // A TS member called `type` or `match` would not parse as a vilan function
    // name; the extern keeps the exact JS spelling either way.
    let output = bind("interface A { type: string; match(): void; }");
    assert!(output.contains("[extern(get, \"type\")]"), "{output}");
    assert!(
        output.contains("external fun type_(self): str;"),
        "{output}"
    );
    assert!(
        output.contains("external fun match_(self): void;"),
        "{output}"
    );
}

#[test]
fn a_name_collision_gets_a_deterministic_suffix() {
    // A property `align` and a method `setAlign` both want `set_align`, and
    // vilan allows one function per name. The suffix follows source order, so
    // it is stable across runs.
    let output = bind("interface C { align: string; setAlign(v: string): void; }");
    assert!(
        output.contains("external fun set_align(self, value: str): void;"),
        "{output}"
    );
    assert!(
        output.contains("external fun set_align_2(self, v: str): void;"),
        "{output}"
    );
}

// --- The generated file as a whole -------------------------------------------

#[test]
fn nothing_is_dropped_silently() {
    // The document's central design commitment (§3). Every member of this
    // interface is either bound or named in a TODO — none simply vanishes.
    let source = "interface Everything {\n\
        ok: string;\n\
        weird: symbol;\n\
        [key: string]: unknown;\n\
        (call: string): void;\n\
    }";
    let generated = generate(source, &options());
    assert!(
        generated.coverage.total_todos() >= 3,
        "{}",
        generated.source
    );
    for marker in ["symbol", "string index signature", "call signature"] {
        assert!(generated.source.contains(marker), "{}", generated.source);
    }
}

#[test]
fn coverage_counts_what_was_bound_and_what_was_not() {
    let generated = generate(
        "interface A { ok: string; [key: string]: number; }",
        &options(),
    );
    assert_eq!(generated.coverage.declarations, 1);
    assert_eq!(generated.coverage.declarations_bound, 1);
    assert_eq!(generated.coverage.members, 2);
    assert_eq!(generated.coverage.members_bound, 1);
    assert!(
        generated
            .coverage
            .report()
            .contains("string index signature")
    );
}

#[test]
fn an_empty_input_still_produces_a_readable_header() {
    let output = bind("");
    assert!(output.contains("Generated by `vilan bindgen`"), "{output}");
    assert!(output.contains("TODO(bindgen)"), "{output}");
}

// --- §6 the three gates ------------------------------------------------------

fn fixture_pairs() -> Vec<(PathBuf, PathBuf)> {
    let directory = fixtures_directory();
    let mut pairs = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("bindgen fixtures directory")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if let Some(stem) = name.strip_suffix(".d.ts") {
            pairs.push((path.clone(), directory.join(format!("{stem}.vl"))));
        }
    }
    assert!(!pairs.is_empty(), "no bindgen fixtures found");
    pairs
}

#[test]
fn every_fixture_matches_its_golden_byte_for_byte() {
    for (input, golden) in fixture_pairs() {
        let source = std::fs::read_to_string(&input).expect("read fixture");
        let generated = generate(
            &source,
            &Options {
                platform: "node".to_string(),
                source_name: input.file_name().unwrap().to_string_lossy().into_owned(),
            },
        );
        let expected = std::fs::read_to_string(&golden).unwrap_or_default();
        assert_eq!(
            generated.source,
            expected,
            "bindgen output changed for {}.\nRegenerate ONLY after confirming the new output is \
             correct:\n  vilan bindgen {} --platform node --stdout > {}",
            input.display(),
            input.display(),
            golden.display()
        );
    }
}

#[test]
fn regenerating_the_same_input_is_byte_stable() {
    // §6 gate 3, and a design constraint on the emitter rather than an
    // afterthought: every synthesized name and every collision suffix has to
    // derive from the input's own structure, not from traversal order.
    for (input, _) in fixture_pairs() {
        let source = std::fs::read_to_string(&input).expect("read fixture");
        let first = generate(&source, &options()).source;
        let second = generate(&source, &options()).source;
        assert_eq!(first, second, "{} regenerated differently", input.display());
    }
}

#[test]
fn every_golden_is_already_formatted() {
    // The emitter pipes everything through `vilan fmt`'s own formatter (§1), so
    // a golden that is not format-stable means the formatter DECLINED to print
    // some construct — which is exactly the signal worth catching.
    for (_, golden) in fixture_pairs() {
        let source = std::fs::read_to_string(&golden).expect("read golden");
        assert_eq!(
            vilan_core::formatter::format(&source),
            source,
            "{} is not formatted",
            golden.display()
        );
    }
}

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Compiles `source` the way the docs gate does, returning its diagnostics.
fn compile(source: &str) -> Vec<String> {
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    let (program, errors) = analyze_source(
        leaked,
        &std_spec(),
        Path::new("."),
        Path::new("bindings.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    match program {
        Some(program) if errors.is_empty() => match transform(&program, &BuildOptions::default()) {
            Ok(_) => Vec::new(),
            Err(error) => vec![error.msg],
        },
        _ => errors.into_iter().map(|error| error.msg).collect(),
    }
}

#[test]
fn every_golden_compiles() {
    // §6 gate 2 — the gate that catches a mapping row that looks right on paper
    // but produces a signature the analyzer rejects. A bindings module has no
    // entry point of its own, so one is appended.
    for (_, golden) in fixture_pairs() {
        let source = std::fs::read_to_string(&golden).expect("read golden");
        let errors = compile(&format!("{source}\nfun main() {{}}\n"));
        assert!(
            errors.is_empty(),
            "{} does not compile: {errors:?}",
            golden.display()
        );
    }
}

// --- The owner-note pins on LANGUAGE facts -----------------------------------

#[test]
fn a_vilan_enum_cannot_carry_a_string_backing_value() {
    // OWNER NOTE 1. bindgen generates a match-wrapper for a string-literal union
    // because a BACKED enum — `enum Align { Start = "start" }` — would be the
    // natural target and does not exist: the discriminant grammar is
    // `= (-)? integer` only (`parsing.rs::parse_discriminant`). A NUMERIC enum
    // already lowers to its bare discriminant, so a string-backed one would
    // lower to its bare string and need no wrapper at all.
    //
    // If the language gains backed enums, this test goes RED and points
    // straight at the wrapper machinery that should then be deleted.
    let errors = compile("enum Align { Start = \"start\", End = \"end\" }\nfun main() {}\n");
    assert!(
        !errors.is_empty(),
        "vilan now accepts a string-backed enum — bindgen's match-wrapper (§3.3) should be \
         replaced by a backed enum, and `proposal/bindgen.md`'s implementation note updated"
    );
    // The integer form is what the language actually has.
    assert!(compile("enum Ordering { Less = -1, Equal = 0 }\nfun main() {}\n").is_empty());
}

#[test]
fn a_vilan_struct_is_a_positional_array_at_runtime() {
    // The representation fact the whole mapping table rests on, pinned rather
    // than asserted: a plain `struct` lowers to `[field, …]` and `p.x` to
    // `p[0]`, so a host object `{x: 1}` read through one yields `undefined`.
    // This is WHY every interface becomes an `external struct` (§3.8), why the
    // discriminated-union row is a TODO, and why `Map`/`List` cannot stand in
    // for index signatures.
    //
    // If vilan ever gives structs a named-field representation, this goes red —
    // and several TODO rows become real mappings.
    let source = "struct Point { x: f64 }\nfun main() { let p = Point { x = 1.0 }; print_f(p.x); }\n\
                  external fun print_f(value: f64): void;\n";
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    let (program, errors) = analyze_source(
        leaked,
        &std_spec(),
        Path::new("."),
        Path::new("repr.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    assert!(errors.is_empty(), "{errors:?}");
    let javascript =
        transform(&program.expect("program"), &BuildOptions::default()).expect("transform");
    assert!(
        javascript.contains("[ 1.0 ]") || javascript.contains("[1.0]"),
        "a struct should lower to a positional array, got:\n{javascript}"
    );
    assert!(
        javascript.contains("p[0]") || javascript.contains("[ 1.0 ][0]"),
        "a field read should lower to a positional index, got:\n{javascript}"
    );
}

#[test]
fn a_transparent_alias_is_substituted_rather_than_lost() {
    // §5: vilan has no type alias, so `type GLenum = number` cannot be
    // DECLARED — but it can be resolved through, which is what §5's "type
    // aliases of mappable shapes" asks for. Before this, `lib.dom.d.ts` alone
    // reported ~1500 references to types it plainly declares.
    let output = bind("type GLenum = number;\ninterface GL { clear(mask: GLenum): void; }");
    assert!(
        output.contains("external fun clear(self, mask: f64): void;"),
        "{output}"
    );
    assert!(!output.contains("is not declared in this file"), "{output}");
    assert!(output.contains("transparent alias"), "{output}");
}

#[test]
fn a_generic_transparent_alias_substitutes_its_arguments() {
    let output = bind("type Maybe<T> = T | null;\ninterface A { a: Maybe<string>; }");
    assert!(output.contains("external fun a(self): str;"), "{output}");
    assert!(
        output.contains("may be `null`/`undefined` at the host"),
        "{output}"
    );
}

#[test]
fn a_cyclic_alias_terminates_instead_of_recursing_forever() {
    let output = bind("type Loop = Loop;\ninterface A { a: Loop; }");
    assert!(output.contains("cyclic type alias"), "{output}");
    assert!(output.contains("external fun a(self): any;"), "{output}");
}

#[test]
fn a_string_literal_variant_that_starts_with_a_digit_stays_a_valid_identifier() {
    // `type OffscreenRenderingContextId = "2d" | "webgl"` is real
    // (`lib.dom.d.ts`), and `2d` is not an identifier. Found by the E31 probe:
    // it was the ONE construct that stopped 410k generated lines from parsing.
    let output = bind("type Ctx = \"2d\" | \"webgl\";\ninterface C { use(c: Ctx): void; }");
    assert!(output.contains("\t_2d,"), "{output}");
    assert!(output.contains("Ctx::_2d => \"2d\","), "{output}");
    assert!(
        compile(&format!("{output}\nfun main() {{}}\n")).is_empty(),
        "{output}"
    );
}

#[test]
fn option_cannot_cross_a_host_boundary_in_either_direction() {
    // The LANGUAGE fact behind the §3.2 correction, pinned rather than
    // asserted. `Option` lowers to a tagged array — `Some(v)` is `[0, v]`,
    // `None` is `[1]` — which a third-party host neither produces nor reads:
    //
    //   - reading: a host returning `"hello"` is tested as `value[0] === 0`,
    //     which is `"h" === 0`, so a PRESENT value arrives as `None`;
    //   - writing: `None` reaches the host as the array `[1]`, which for an
    //     optional `boolean` argument is TRUTHY.
    //
    // If `Option` ever gains a host representation, this goes red and §3.2's
    // original mapping becomes correct again.
    let source = "import std::option::Option;\n\
                  import std::option::Option::None;\n\
                  [extern(\"host\")] external fun host(value: Option<str>): str;\n\
                  fun main() { let _ = host(None); }\n";
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    let (program, errors) = analyze_source(
        leaked,
        &std_spec(),
        Path::new("."),
        Path::new("opt.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    assert!(errors.is_empty(), "{errors:?}");
    let javascript =
        transform(&program.expect("program"), &BuildOptions::default()).expect("transform");
    assert!(
        javascript.contains("host([ 1 ])") || javascript.contains("host([1])"),
        "`None` should reach the host as the raw tagged array, got:\n{javascript}"
    );
}
