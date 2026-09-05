//! E124's definition, pinned per CASE rather than per example
//! (`proposal/dead-code-paint.md` §6.1).
//!
//! Every pin here answers one question about `dead_items::paintable_items` and
//! `dead_items::reached_item_keys` composed the way an editor composes them:
//! the candidates of a file, minus the keys the walk from `main` reaches. The
//! three exemptions the paper found by measurement — the `const`
//! module-binding hole, the ambient-context hole, and the whole type-level
//! universe — are each RED without the code that ships beside this file, which
//! is the point of pinning them before any gray reaches a screen.
//!
//! The `generated`-root exemption is not here: it is a manifest fact, so it is
//! pinned where a manifest exists (`vilan-lsp`'s paint pins).

use std::path::{Path, PathBuf};

use vilan_core::analyzer::SourceId;
use vilan_core::dead_items::{ItemKey, paintable_items, reached_item_keys};
use vilan_core::{Platform, Workspace, analyze_source};

fn std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// The names this program's entry file would be painted gray — the paintable
/// top-level items of the entry, minus everything the walk from `main` reaches.
///
/// On the big stack the pipeline needs (the analyzer recurses, and a derive
/// nests a whole analysis inside this one), and computed on the worker thread:
/// only the names travel back.
fn grays(source: &'static str) -> Vec<String> {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = vilan_core::manifest::resolve_std(&std_root());
            let (program, errors) = analyze_source(
                source,
                &std,
                Path::new("."),
                Path::new("dead_items_probe.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("the probe analyzes");
            assert!(errors.is_empty(), "the probe compiles cleanly: {errors:?}");
            let reached = reached_item_keys(&program)
                .expect("the probe declares a `main`, so the walk has a root");
            let path = program.canonical_sources[0].clone();
            let mut names: Vec<String> = paintable_items(&program, SourceId(0))
                .into_iter()
                .filter(|item| {
                    !reached.contains(&ItemKey {
                        path: path.clone(),
                        name_span: item.name_span,
                    })
                })
                .map(|item| item.name)
                .collect();
            names.sort();
            names
        })
        .expect("spawn the probe thread")
        .join()
        .expect("the probe thread joins")
}

/// The names the paint OFFERS at all, reached or not — the candidate set, which
/// is what the type-level pin has to read: a `struct` that is never gray
/// because it is never a candidate is a different fact from one that is never
/// gray because it is always reached, and only the first is E124's.
fn candidates(source: &'static str) -> Vec<String> {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = vilan_core::manifest::resolve_std(&std_root());
            let (program, errors) = analyze_source(
                source,
                &std,
                Path::new("."),
                Path::new("dead_items_probe.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("the probe analyzes");
            assert!(errors.is_empty(), "the probe compiles cleanly: {errors:?}");
            let mut names: Vec<String> = paintable_items(&program, SourceId(0))
                .into_iter()
                .map(|item| item.name)
                .collect();
            names.sort();
            names
        })
        .expect("spawn the probe thread")
        .join()
        .expect("the probe thread joins")
}

/// Pins 1 and 2 — the paint working. A top-level `fun` nothing reaches and a
/// module-level `let` nothing reads are gray; the ones `main` reaches are not.
#[test]
fn an_unreached_fun_and_an_unreached_module_let_are_gray() {
    let grays = grays(
        "let used_binding: i32 = 3;\n\
         let dead_binding: i32 = 4;\n\
         fun used(): i32 { used_binding }\n\
         fun dead(): i32 { 9 }\n\
         fun main() { print(i\"{used()}\"); }\n",
    );
    assert_eq!(
        grays,
        vec!["dead".to_string(), "dead_binding".to_string()],
        "exactly the two unreached items gray"
    );
}

/// Pin 3, the both-directions pin — a `struct`, an `enum` and a `trait` are
/// never CANDIDATES, used or unused. The failure this guards is silent and
/// total: types emit nothing whether they are used or not (`dead-code-paint.md`
/// §1.2, probe P2 — a used `Point { x = 1, y = 2 }` emits `[ 1, 2 ]` with no
/// declaration anywhere), so a paint that read emission would gray every type
/// in the language.
#[test]
fn no_type_declaration_is_ever_a_paint_candidate() {
    let unused = candidates(
        "struct Unused { x: i32 }\n\
         enum Color { Red, Blue }\n\
         trait Greet { fun greet(self): str; }\n\
         fun main() { print(\"hi\"); }\n",
    );
    assert!(
        unused.is_empty(),
        "no type declaration is a candidate, used or not: {unused:?}"
    );
    let used = candidates(
        "struct Point { x: i32, y: i32 }\n\
         fun main() {\n\
         \tlet p = Point { x = 1, y = 2 };\n\
         \tprint(i\"{p.x}\");\n\
         }\n",
    );
    assert!(
        used.is_empty(),
        "a USED type is not a candidate either: {used:?}"
    );
}

/// Pin 6, the `const` module-binding hole — RED before `CallGraph`'s
/// paint-only const edges.
///
/// `CallGraph::build` deliberately drops the edges out of a `const`-marked
/// module binding, because at run time such an initializer is data, not code.
/// That is right for emission and wrong for paint: `table_row` is unreached,
/// unemitted — and deleting it breaks the build (§1.6, probe P3). On kolt this
/// one hole was 27 of the 1,859 grays, 26 of them in a single file.
#[test]
fn a_callee_reached_only_from_a_const_module_binding_is_not_gray() {
    let grays = grays(
        "fun table_row(n: i32): i32 { n * n }\n\
         let squares: i32 = const table_row(7);\n\
         fun main() { print(i\"{squares}\"); }\n",
    );
    assert!(
        grays.is_empty(),
        "`table_row` is called only from a `const` initializer and its deletion \
         breaks the build, so it must not gray: {grays:?}"
    );
}

/// Pin 7, the already-safe half — a `const` expression inside a function BODY
/// still contributes its call edge, because `CallGraph::build` runs before
/// `const_eval::evaluate`. Pinned so a reordering of `post_analysis_passes`
/// cannot silently break it.
#[test]
fn a_callee_reached_only_from_a_const_expression_in_a_body_is_not_gray() {
    let grays = grays(
        "fun folded_in_body(n: i32): i32 { n + 1 }\n\
         fun main() {\n\
         \tlet answer: i32 = const folded_in_body(41);\n\
         \tprint(i\"{answer}\");\n\
         }\n",
    );
    assert!(
        grays.is_empty(),
        "an in-body const callee is reached by the graph even though the \
         emitter folds it away: {grays:?}"
    );
}

/// Pin 8, the `context` hole — RED before `Program::context_bindings`.
///
/// `context::thread_contexts` REWRITES the program: an ambient read stops being
/// a read of the binding and becomes a hidden parameter, so the graph the walk
/// uses has no reader of `app_context` left. The walk is not wrong about the
/// bundle — the emitted client contains no `Context` at all — but the
/// declaration is the shipped ambient-owner idiom, and graying it tells the
/// user to delete the thing the file exists for (§1.7).
#[test]
fn a_binding_the_context_pass_rewrites_away_is_not_gray() {
    let grays = grays(
        "import std::context::Context;\n\
         \n\
         struct AppState { count: i32 }\n\
         \n\
         let app_context: Context<AppState> = Context<AppState>::new();\n\
         \n\
         fun read_count(): i32 { app_context.get().count }\n\
         \n\
         fun main() {\n\
         \tapp_context.run(AppState { count = 7 }, || {\n\
         \t\tprint(i\"{read_count()}\");\n\
         \t});\n\
         }\n",
    );
    assert!(
        grays.is_empty(),
        "the ambient context binding is load-bearing source however the \
         rewrite leaves the graph: {grays:?}"
    );
}

/// Pin 11's v1 shape — a trait impl member is never painted, whichever types
/// the program constructs.
///
/// The walk's dispatch refinement is per instantiation: construct only a `Sq`
/// and `Ci::area` is genuinely unreached (§1.8, probe P4). That is correct by
/// the definition and it is the definition's sharpest edge — constructing one
/// `Ci` anywhere in any entry makes a dozen grays vanish at once — so v1 leaves
/// the whole class unpainted rather than shipping the jumpiest true gray it
/// has. An INHERENT impl member stays paintable, which is the pin's other half.
#[test]
fn a_trait_impl_member_is_unpainted_and_an_inherent_one_is_not() {
    let names = grays(
        "trait Shape { fun area(self): i32; }\n\
         \n\
         struct Sq { side: i32 }\n\
         struct Ci { r: i32 }\n\
         \n\
         impl Sq with Shape {\n\
         \tfun area(self): i32 { self.side * self.side }\n\
         }\n\
         \n\
         impl Ci with Shape {\n\
         \tfun area(self): i32 { self.r * self.r * 3 }\n\
         }\n\
         \n\
         impl Sq {\n\
         \tfun perimeter(self): i32 { self.side * 4 }\n\
         }\n\
         \n\
         fun main() {\n\
         \tlet s = Sq { side = 3 };\n\
         \tprint(i\"{s.area()}\");\n\
         }\n",
    );
    assert_eq!(
        names,
        vec!["perimeter".to_string()],
        "`Ci::area` is a trait impl member and stays unpainted in v1; the \
         inherent `Sq::perimeter` is a true find and grays"
    );
}

/// Pin 5 — a derive-generated member never produces a range in a user file, so
/// a file-scoped paint cannot reach one even by accident. Those entities carry
/// `DERIVED_SOURCE`, which is outside `sources`.
#[test]
fn a_derive_generated_member_is_not_a_candidate_in_the_user_file() {
    let names = candidates(
        "[derive(PartialEq)]\n\
         struct Pair { a: i32, b: i32 }\n\
         \n\
         fun main() {\n\
         \tlet one = Pair { a = 1, b = 2 };\n\
         \tlet two = Pair { a = 1, b = 2 };\n\
         \tprint(i\"{one == two}\");\n\
         }\n",
    );
    assert!(
        names.is_empty(),
        "the derive's `eq` lives in DERIVED_SOURCE and never lands in the \
         user file's id range: {names:?}"
    );
}

/// An `_`-led name is the language's own "I know" marker, and E114's locals
/// paint already reads it that way. kolt's `let _page_defaults = const
/// page_defaults();` is `_`-led AND const, and the `_` alone should keep it
/// quiet.
#[test]
fn an_underscore_led_top_level_item_is_never_a_candidate() {
    let names = candidates(
        "let _unused_on_purpose: i32 = 5;\n\
         fun _also_on_purpose(): i32 { 6 }\n\
         fun main() { print(\"hi\"); }\n",
    );
    assert!(
        names.is_empty(),
        "an `_`-led top-level item is exempt by its name: {names:?}"
    );
}

/// Pin 4, both states (§6.1) — an `[rpc]` method is reached through the
/// `dispatcher(self)` the analyzer synthesizes for a `[service]`, so nothing
/// here is exempt and nothing here is special-cased: the WALK answers it. A
/// service some entry installs has live methods; the same method on a service
/// no entry installs is genuinely dead, and grays.
///
/// The two states share one exhibit and differ by one line, which is what makes
/// this one pin rather than two: `main` either reaches the dispatcher or does
/// not.
#[test]
fn an_rpc_method_is_reached_through_the_dispatcher_and_gray_without_it() {
    const INSTALLED: &str = "import std::reactive::{ Signal, SignalCell };\n\
         \n\
         [service(NotesClient)]\n\
         struct Notes {\n\t[expose] count: SignalCell<i32>,\n}\n\
         \n\
         impl Notes {\n\
         \t[rpc]\n\
         \tfun bump(self): i32 { self.count.get() + 1 }\n\
         }\n\
         \n\
         fun main() {\n\
         \tlet notes = Notes { count = Signal::new(1) };\n\
         \tlet installed = notes.dispatcher();\n\
         }\n";
    const UNINSTALLED: &str = "import std::reactive::{ Signal, SignalCell };\n\
         \n\
         [service(NotesClient)]\n\
         struct Notes {\n\t[expose] count: SignalCell<i32>,\n}\n\
         \n\
         impl Notes {\n\
         \t[rpc]\n\
         \tfun bump(self): i32 { self.count.get() + 1 }\n\
         }\n\
         \n\
         fun main() {\n\
         \tlet notes = Notes { count = Signal::new(1) };\n\
         }\n";
    assert!(
        !grays(INSTALLED).contains(&"bump".to_string()),
        "an `[rpc]` method of an INSTALLED service is reached through the \
         synthesized dispatcher: {:?}",
        grays(INSTALLED),
    );
    assert!(
        grays(UNINSTALLED).contains(&"bump".to_string()),
        "the same method on a service no entry installs is genuinely dead: {:?}",
        grays(UNINSTALLED),
    );
}

/// Pin 11, in its two-state form, and the DECISION it records (E140).
///
/// The paper asked for one pin over two states: a trait-impl member for a type
/// constructed nowhere is gray, and stops being gray the moment any entry
/// constructs the type. E124 shipped v1 with the whole class unpainted, so the
/// two-state pin as written would have contradicted the code — and E140 asked
/// for the question to be decided rather than left open.
///
/// **Decided: a trait-impl member never paints, and not in this order.** The
/// state below is what makes it more than a preference. The walk's dispatch
/// refinement is per INSTANTIATION, so `Ci::area`'s grayness is a fact about
/// whether any entry anywhere constructs a `Ci` — and a package's entries are
/// analyzed on an idle clock the user cannot see. Constructing one `Ci` in any
/// entry makes every one of its trait members un-gray at once, and deleting the
/// last construction grays a dozen members in files nobody touched. That is
/// correct by the definition and it is the jumpiest true gray the definition
/// has; a paint whose answer moves in blocks, on a clock, in files the user is
/// not editing, is the class most likely to read as noise. Painting it would
/// want a different presentation (a report, not an inline fade) and a different
/// question (which types are constructed), and neither is E124's.
///
/// So the two states are pinned as they SHIP: neither paints, and the inherent
/// member beside them paints in both — which is what keeps this a decision
/// about trait-impl members rather than a paint that has quietly stopped
/// working.
#[test]
fn a_trait_impl_member_paints_in_neither_state_and_an_inherent_one_paints_in_both() {
    const UNCONSTRUCTED: &str = "trait Shape { fun area(self): i32; }\n\
         \n\
         struct Sq { side: i32 }\n\
         struct Ci { r: i32 }\n\
         \n\
         impl Sq with Shape {\n\
         \tfun area(self): i32 { self.side * self.side }\n\
         }\n\
         \n\
         impl Ci with Shape {\n\
         \tfun area(self): i32 { self.r * self.r * 3 }\n\
         }\n\
         \n\
         impl Ci {\n\
         \tfun width(self): i32 { self.r * 2 }\n\
         }\n\
         \n\
         fun main() {\n\
         \tlet s = Sq { side = 3 };\n\
         \tprint(i\"{s.area()}\");\n\
         }\n";
    /// The same program with `Ci` CONSTRUCTED — the state change pin 11 is
    /// about, and the only line that differs.
    const CONSTRUCTED: &str = "trait Shape { fun area(self): i32; }\n\
         \n\
         struct Sq { side: i32 }\n\
         struct Ci { r: i32 }\n\
         \n\
         impl Sq with Shape {\n\
         \tfun area(self): i32 { self.side * self.side }\n\
         }\n\
         \n\
         impl Ci with Shape {\n\
         \tfun area(self): i32 { self.r * self.r * 3 }\n\
         }\n\
         \n\
         impl Ci {\n\
         \tfun width(self): i32 { self.r * 2 }\n\
         }\n\
         \n\
         fun main() {\n\
         \tlet s = Sq { side = 3 };\n\
         \tlet c = Ci { r = 1 };\n\
         \tprint(i\"{s.area()}\");\n\
         }\n";
    for (label, source) in [
        ("`Ci` constructed nowhere", UNCONSTRUCTED),
        ("`Ci` constructed by the entry", CONSTRUCTED),
    ] {
        let names = grays(source);
        assert!(
            !names.contains(&"area".to_string()),
            "{label}: a trait-impl member is unpainted in BOTH states: {names:?}",
        );
        assert!(
            names.contains(&"width".to_string()),
            "{label}: the inherent member beside it is a true find and grays, so \
             the pin above is about the class rather than about a paint that \
             stopped working: {names:?}",
        );
    }
}
