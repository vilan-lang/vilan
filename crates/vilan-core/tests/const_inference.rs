//! The inferred-`const` sweep (proposal/const-eval.md §9): `let a = 1 + 2;`
//! folding without the keyword, under the release preset only, and falling
//! back to runtime — SILENTLY, with zero diagnostics — on every failure.
//!
//! These pins are stated over `const_eval::infer`'s own result map rather than
//! over emitted JavaScript, because the interesting cases are the ones that do
//! NOT fold, and a binding that did not fold looks in the output exactly like a
//! binding nobody swept. Asking the sweep directly is the only way to tell "left
//! alone" from "never looked at". The emitted-JS half of the contract is
//! `vilan-cli/tests/infer_differential.rs` (observational equivalence over the
//! whole corpus) and the `infer-preset` golden pair.

use std::path::{Path, PathBuf};

use vilan_core::analyzer::Program;
use vilan_core::interpreter::ConstValue;
use vilan_core::options::{BuildOptions, Preset};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Analyzes a source on a big-stack worker (the pipeline recurses deeply) and
/// returns the program, asserting it compiled clean — every pin here is about
/// a program that BUILDS, since the sweep never runs on one that does not.
fn analyze(source: &str) -> Program<'static> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(
                errors.is_empty(),
                "the source must compile clean: {errors:?}"
            );
            program.expect("analyzed")
        })
        .expect("spawn worker")
        .join()
        .expect("worker")
}

fn release() -> BuildOptions {
    BuildOptions::from_preset(Preset::Release)
}

/// The id of the ENTRY's binding called `name`. Scoped to `SourceId(0)` — the
/// entry file — because std is analyzed into the same program and shares plenty
/// of ordinary identifiers with a test fixture; an unscoped search finds a std
/// binding and silently pins the wrong thing.
fn initializer_of(program: &Program, name: &str) -> vilan_core::id::Id {
    program
        .variables
        .values()
        .find(|variable| {
            variable.name == name
                && program
                    .source_of(variable.id)
                    .is_some_and(|source| source.0 == 0)
        })
        .unwrap_or_else(|| panic!("no binding named `{name}` in the entry"))
        .initial
        .unwrap_or_else(|| panic!("`{name}` has no initializer"))
}

/// The value the sweep folded `name`'s initializer to, or `None` if it left the
/// binding alone.
fn folded(program: &Program, options: &BuildOptions, name: &str) -> Option<ConstValue> {
    let results = vilan_core::const_eval::infer(program, options);
    results.get(&initializer_of(program, name)).cloned()
}

/// `folded`, for a program whose bindings are swept once and asked about
/// several times.
fn fold_of<'a>(
    program: &Program,
    results: &'a vilan_core::fx::FxHashMap<vilan_core::id::Id, ConstValue>,
    name: &str,
) -> Option<&'a ConstValue> {
    results.get(&initializer_of(program, name))
}

// ---------------------------------------------------------------------------
// The positive controls: what inference is for.
// ---------------------------------------------------------------------------

/// The headline case from the backlog entry: `let a = 1 + 2;` folds with no
/// keyword.
#[test]
fn a_plain_arithmetic_initializer_folds() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tlet a = 1 + 2 * 3;\n\
         \tprint(a);\n\
         }\n\
         main();\n",
    );
    assert_eq!(
        folded(&program, &release(), "a"),
        Some(ConstValue::Number(7.0))
    );
}

/// A call folds too — the free-variable rule constrains *variables*, never
/// calls (§1's no-coloring rule, inherited).
#[test]
fn a_call_with_const_known_arguments_folds() {
    let program = analyze(
        "import std::io::print;\n\
         fun square(n: i32): i32 { n * n }\n\
         fun main() {\n\
         \tlet nine = square(3);\n\
         \tprint(nine);\n\
         }\n\
         main();\n",
    );
    assert_eq!(
        folded(&program, &release(), "nine"),
        Some(ConstValue::Number(9.0))
    );
}

/// §9.5's chaining rule: a candidate whose free variable is ANOTHER candidate
/// folds, because in the inferred mode a pending candidate counts as
/// compile-time-known. Without it `b` would stay runtime and the feature would
/// stop at depth one.
#[test]
fn inference_chains_through_inferred_bindings() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tlet a = 1 + 2;\n\
         \tlet b = a * 2;\n\
         \tlet c = b + a;\n\
         \tprint(c);\n\
         }\n\
         main();\n",
    );
    let results = vilan_core::const_eval::infer(&program, &release());
    assert_eq!(
        fold_of(&program, &results, "a"),
        Some(&ConstValue::Number(3.0))
    );
    assert_eq!(
        fold_of(&program, &results, "b"),
        Some(&ConstValue::Number(6.0))
    );
    assert_eq!(
        fold_of(&program, &results, "c"),
        Some(&ConstValue::Number(9.0))
    );
}

/// `mut` is eligible: §1 already spells "a compile-time initial value for
/// runtime-mutable state" as `mut x = const initial()`, and inference gives the
/// same thing without the keyword. (A `mut` binding remains disqualified as a
/// free variable OF a fold — that is a different rule, pinned below.)
#[test]
fn a_mutable_bindings_initializer_folds() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tmut cache = 10 * 10;\n\
         \tcache = cache + 1;\n\
         \tprint(cache);\n\
         }\n\
         main();\n",
    );
    assert_eq!(
        folded(&program, &release(), "cache"),
        Some(ConstValue::Number(100.0))
    );
}

// ---------------------------------------------------------------------------
// The preset gate (§9.4).
// ---------------------------------------------------------------------------

/// Debug infers NOTHING — folded computation vanishes from stack traces, so the
/// readable build keeps it. This is also what makes the corpus byte-identical
/// by construction: the gate builds through the debug binary with no manifest,
/// and `BuildOptions::default()` IS the debug preset.
#[test]
fn the_debug_preset_folds_nothing() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tlet a = 1 + 2 * 3;\n\
         \tprint(a);\n\
         }\n\
         main();\n",
    );
    let debug = BuildOptions::from_preset(Preset::Debug);
    assert!(
        vilan_core::const_eval::infer(&program, &debug).is_empty(),
        "the debug preset must not fold anything"
    );
    assert_eq!(
        BuildOptions::default(),
        debug,
        "the default must stay the debug preset — a bare `vilan build file.vl` \
         resolves it, and that is what keeps the corpus goldens still"
    );
}

/// The `[build] infer-const` override reaches the sweep from either side, like
/// every other code-generation knob.
#[test]
fn the_manifest_override_reaches_the_sweep_both_ways() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tlet a = 1 + 2 * 3;\n\
         \tprint(a);\n\
         }\n\
         main();\n",
    );
    let mut release_without = release();
    release_without.infer_const = false;
    assert!(
        vilan_core::const_eval::infer(&program, &release_without).is_empty(),
        "`infer-const = false` must switch the sweep off under release"
    );
    let mut debug_with = BuildOptions::from_preset(Preset::Debug);
    debug_with.infer_const = true;
    assert!(
        !vilan_core::const_eval::infer(&program, &debug_with).is_empty(),
        "`infer-const = true` must switch the sweep on under debug"
    );
}

// ---------------------------------------------------------------------------
// The fallback shapes (§9.2). Each asserts BOTH halves: the binding stays
// runtime, AND nothing was reported.
// ---------------------------------------------------------------------------

/// The free-variable rule is the filter (§9.1): an initializer reading a
/// parameter cannot be settled at compile time and is left alone. Under the
/// EXPLICIT form the same shape is a diagnostic; here it is silence.
#[test]
fn an_initializer_reading_a_parameter_falls_back() {
    let program = analyze(
        "import std::io::print;\n\
         fun scale(n: i32): i32 {\n\
         \tlet doubled = n * 2;\n\
         \tdoubled\n\
         }\n\
         fun main() { print(scale(4)); }\n\
         main();\n",
    );
    assert_eq!(folded(&program, &release(), "doubled"), None);
}

/// A `mut` binding disqualifies as a free variable of a fold, exactly as it
/// does for the explicit form (§1) — its value is not fixed at compile time.
#[test]
fn an_initializer_reading_a_mutable_binding_falls_back() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tmut counter = 1;\n\
         \tcounter = counter + 1;\n\
         \tlet derived = counter * 10;\n\
         \tprint(derived);\n\
         }\n\
         main();\n",
    );
    assert_eq!(folded(&program, &release(), "derived"), None);
}

/// An UNSUPPORTED CONSTRUCT — a host capability the const world does not have.
/// The clock is the canonical one and is already pinned for the explicit form
/// (`the_clock_is_not_const_evaluable`); here it must produce silence rather
/// than the explicit form's diagnostic.
#[test]
fn an_initializer_reaching_a_host_capability_falls_back() {
    let program = analyze(
        "import std::io::print;\n\
         import std::time;\n\
         fun main() {\n\
         \tlet started = time::now();\n\
         \tprint(started);\n\
         }\n\
         main();\n",
    );
    assert_eq!(folded(&program, &release(), "started"), None);
}

/// BUDGET EXHAUSTION (§9.3): a loop long enough to blow the inferred fuel cap
/// of 10 000 while producing a single small number, so FUEL is unambiguously
/// what refuses it — the explicit form's 1 000 000 would evaluate the same
/// expression happily, which is the point of having two numbers.
///
/// The iteration count is chosen to sit between the two caps: raising the
/// inferred budget to the explicit one must make this fold.
#[test]
fn an_initializer_over_the_fuel_budget_falls_back() {
    let program = analyze(
        "import std::io::print;\n\
         import std::range::Range;\n\
         fun spin(): i32 {\n\
         \tmut total = 0;\n\
         \tfor index in Range::new(0, 4000) {\n\
         \t\ttotal = total + index;\n\
         \t}\n\
         \ttotal\n\
         }\n\
         fun main() { let heavy = spin(); print(heavy); }\n\
         main();\n",
    );
    assert_eq!(folded(&program, &release(), "heavy"), None);
}

/// THE SIZE CAP (§9.3), and its non-vacuity in the same program: a table that
/// is CHEAP to compute but wide to serialize stays runtime, while a small one
/// folds. Both in one test so the cap is shown to be the discriminator — not
/// "lists never fold" (which one negative pin would leave open) and not the
/// fuel budget (which the element count is kept low enough to clear).
#[test]
fn the_size_cap_refuses_a_wide_result_and_admits_a_small_one() {
    let program = analyze(
        "import std::io::print;\n\
         import std::range::Range;\n\
         fun build(limit: i32, stride: i32): List<i32> {\n\
         \tmut out: List<i32> = List::new();\n\
         \tfor index in Range::new(0, limit) {\n\
         \t\tout.push(index * stride);\n\
         \t}\n\
         \tout\n\
         }\n\
         fun main() {\n\
         \tlet wide = build(40, 1234567);\n\
         \tlet small = build(5, 7);\n\
         \tprint(wide[3] + small[3]);\n\
         }\n\
         main();\n",
    );
    let results = vilan_core::const_eval::infer(&program, &release());
    assert_eq!(
        fold_of(&program, &results, "wide"),
        None,
        "40 eight-digit entries serialize past the 256-byte cap"
    );
    assert_eq!(
        fold_of(&program, &results, "small"),
        Some(&ConstValue::Array(vec![
            ConstValue::Number(0.0),
            ConstValue::Number(7.0),
            ConstValue::Number(14.0),
            ConstValue::Number(21.0),
            ConstValue::Number(28.0),
        ])),
        "a 5-element table is well inside the cap and must fold"
    );
}

/// A PANIC during evaluation falls back rather than erroring (§5's load-bearing
/// case). The subscript is out of bounds, so the program throws at run time —
/// and must go on throwing at run time. Turning that into a compile error would
/// reject a program whose panicking path may never be reached.
#[test]
fn a_panicking_initializer_falls_back_without_erroring() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tlet xs = [1, 2, 3];\n\
         \tlet boom = xs[9];\n\
         \tprint(boom);\n\
         }\n\
         main();\n",
    );
    assert_eq!(folded(&program, &release(), "boom"), None);
}

/// THE EFFECT RULE (§9.2), the hole §5 did not record: the explicit form
/// legitimately swallows what a `const` evaluation printed — you asked for the
/// computation to move to compile time — but an inferred fold is invisible, so
/// swallowing a `print` would silently delete output from a working program
/// when someone switched preset.
#[test]
fn a_printing_initializer_falls_back() {
    let program = analyze(
        "import std::io::print;\n\
         fun noisy(): i32 {\n\
         \tprint(\"side effect!\");\n\
         \t7\n\
         }\n\
         fun main() {\n\
         \tlet value = noisy();\n\
         \tprint(value);\n\
         }\n\
         main();\n",
    );
    assert_eq!(
        folded(&program, &release(), "value"),
        None,
        "folding this would delete `side effect!` from the program's output"
    );
}

/// CONST-ONLY FUNCTIONS NEVER INFER (§5, §9.2). `styled` reaches
/// `asset::emit`; the explicit `const` at the bottom is what makes the program
/// legal, and `wrapper`'s own `let` must stay runtime — inference folds values,
/// it never creates const contexts. Note what is NOT asserted: no diagnostic.
/// The sweep never reports, so this shape is a fallback, not an error.
#[test]
fn an_emit_reaching_initializer_falls_back_without_erroring() {
    let program = analyze(
        "import std::io::print;\n\
         import std::asset;\n\
         fun styled(): i32 {\n\
         \tasset::emit(\"css\", \".x{color:red}\");\n\
         \t1\n\
         }\n\
         fun wrapper(): i32 {\n\
         \tlet inner = styled();\n\
         \tinner + 1\n\
         }\n\
         fun main() {\n\
         \tlet ok = const wrapper();\n\
         \tprint(ok);\n\
         }\n\
         main();\n",
    );
    assert_eq!(
        folded(&program, &release(), "inner"),
        None,
        "an inferred attempt runs with the asset channel closed, so reaching \
         `asset::emit` is a capability miss"
    );
}

/// A TYPE-PARAMETER-DEPENDENT CONTEXT (§9.1, and §5's "const generics are out
/// of scope"): a binding inside a generic function body has no monomorphization
/// in the const mini-program, so a fold there is not merely unsound, it is
/// SILENTLY unsound — `T::default()` evaluates to `undefined` rather than
/// failing.
///
/// This is the shape the corpus differential caught on `list-element-type.vl`:
/// `List<T>::sum` opens with `let total = T::default();`, which folded to
/// `undefined` and made the program print `undefined` where it printed `0`. The
/// fixture below is that program in miniature, and `first` is asserted to fold
/// so the test also shows the exclusion is scoped to the generic body rather
/// than switching inference off wholesale.
#[test]
fn a_binding_inside_a_generic_function_never_folds() {
    let program = analyze(
        "import std::io::print;\n\
         import std::default::Default;\n\
         fun head_or_default<T: Default>(items: List<T>): T {\n\
         \tlet fallback = T::default();\n\
         \tmut found = fallback;\n\
         \tfor item in items {\n\
         \t\tfound = item;\n\
         \t}\n\
         \tfound\n\
         }\n\
         fun main() {\n\
         \tlet first = 2 + 3;\n\
         \tlet empty: List<i32> = List::new();\n\
         \tprint(head_or_default(empty) + first);\n\
         }\n\
         main();\n",
    );
    let results = vilan_core::const_eval::infer(&program, &release());
    assert_eq!(
        fold_of(&program, &results, "fallback"),
        None,
        "`T::default()` has no meaning without a monomorphization: folding it \
         yields `undefined`, not a diagnostic"
    );
    assert_eq!(
        fold_of(&program, &results, "first"),
        Some(&ConstValue::Number(5.0)),
        "the exclusion must be scoped to the generic body, not switch the sweep \
         off for the whole program"
    );
}

/// The same rule reaching the case the function's OWN generic parameters do not
/// name: a method on a generic type. `List<T>::sum` has no type parameters of
/// its own — `T` belongs to `List` — so a check that looked only at
/// `generic_parameter_constraint_ids` would miss it entirely. Stated over the
/// real std method the differential caught.
#[test]
fn a_method_on_a_generic_type_is_also_excluded() {
    let program = analyze(
        "import std::io::print;\n\
         fun main() {\n\
         \tlet empty: List<i32> = List::new();\n\
         \tprint(empty.sum());\n\
         }\n\
         main();\n",
    );
    let results = vilan_core::const_eval::infer(&program, &release());
    let leaked_into_a_generic_method = results.keys().any(|id| {
        program.variables.values().any(|variable| {
            variable.initial == Some(*id) && (variable.name == "total" || variable.name == "seeded")
        })
    });
    assert!(
        !leaked_into_a_generic_method,
        "the sweep folded inside `List<T>::sum`, whose `T` comes from the \
         RECEIVER type and not from the method's own generic parameters \
         (const-eval.md §9.1)"
    );
}

// ---------------------------------------------------------------------------
// The invariants the whole design rests on.
// ---------------------------------------------------------------------------

/// SILENT FALLBACK, stated over every shape at once (§9.2): a program full of
/// things that cannot fold leaves every one of them alone.
///
/// The "reports nothing" half is not assertable from out here and deliberately
/// so — `infer` takes `&Program` and returns only a map, so it *cannot* push a
/// diagnostic; the shape of the signature is the guarantee. What backs it up is
/// the `debug_assert!` inside `infer` that its internal error list came out
/// empty, which these (debug-profile) tests execute: making `report` report in
/// the inferred mode panics this test rather than passing it.
#[test]
fn the_sweep_never_produces_a_diagnostic() {
    let program = analyze(
        "import std::io::print;\n\
         import std::time;\n\
         fun scale(n: i32): i32 { let doubled = n * 2; doubled }\n\
         fun noisy(): i32 { print(\"x\"); 7 }\n\
         fun main() {\n\
         \tmut counter = 1;\n\
         \tcounter = counter + 1;\n\
         \tlet derived = counter * 10;\n\
         \tlet started = time::now();\n\
         \tlet loud = noisy();\n\
         \tlet xs = [1, 2, 3];\n\
         \tlet boom = xs[9];\n\
         \tprint(scale(derived) + started + loud + boom);\n\
         }\n\
         main();\n",
    );
    let results = vilan_core::const_eval::infer(&program, &release());
    for name in ["derived", "started", "loud", "boom"] {
        assert_eq!(
            fold_of(&program, &results, name),
            None,
            "`{name}` cannot be settled at compile time and must be left alone"
        );
    }
    assert!(
        program.diagnostics.is_empty(),
        "the fixture must analyze clean, or the sweep would not have run at all"
    );
}

/// DETERMINISM (§9.5): the same program swept twice folds identically. The
/// sweep visits candidates in source order rather than `HashMap` order, so this
/// cannot come down to a hash seed.
#[test]
fn folding_is_identical_across_repeated_sweeps() {
    let program = analyze(
        "import std::io::print;\n\
         fun square(n: i32): i32 { n * n }\n\
         fun main() {\n\
         \tlet a = 1 + 2;\n\
         \tlet b = a * 2;\n\
         \tlet c = square(b);\n\
         \tlet d = [a, b, c];\n\
         \tprint(c + d[0]);\n\
         }\n\
         main();\n",
    );
    // Compared as sorted text, not as maps: std folds `f64::NAN` among its
    // constants, and `NaN != NaN` would fail a structural comparison of two
    // genuinely identical results.
    let canonical = |results: vilan_core::fx::FxHashMap<vilan_core::id::Id, ConstValue>| {
        let mut rendered: Vec<String> = results
            .into_iter()
            .map(|(id, value)| format!("{}={value:?}", id.0))
            .collect();
        rendered.sort();
        rendered
    };
    let first = canonical(vilan_core::const_eval::infer(&program, &release()));
    let second = canonical(vilan_core::const_eval::infer(&program, &release()));
    assert!(
        !first.is_empty(),
        "the fixture must actually fold something"
    );
    assert_eq!(first, second, "two sweeps of one program must agree");
}

/// THE ASYMMETRY THE PRESET SPLIT MUST NEVER BREAK (§9.5): the inferred mode
/// treats a pending candidate as compile-time-known; the EXPLICIT mode must
/// not. If it did, `const doubled` below would compile under release and fail
/// under debug — the same program accepted by one preset and rejected by the
/// other, which is the one thing a code-generation knob may never do.
#[test]
fn an_explicit_const_still_rejects_a_plain_runtime_binding() {
    let source = "import std::io::print;\n\
                  fun main() {\n\
                  \tlet base = 2 + 3;\n\
                  \tlet doubled = const base * 2;\n\
                  \tprint(doubled);\n\
                  }\n\
                  main();\n";
    let source = source.to_string();
    let errors = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (_, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            errors
        })
        .expect("spawn worker")
        .join()
        .expect("worker");
    assert!(
        errors
            .iter()
            .any(|error| error.msg.contains("`base` is a runtime value")),
        "the explicit form must keep rejecting a plain runtime free variable, \
         whatever the preset: {errors:?}"
    );
}

/// The size metric the cap is stated in tracks the emitted literal
/// (`transformer::const_value_to_js` printed tight), including the escape
/// expansion a naive byte count would miss.
#[test]
fn the_literal_size_metric_matches_the_emitted_form() {
    assert_eq!(ConstValue::Number(7.0).literal_size(), 1);
    assert_eq!(ConstValue::Number(1234.0).literal_size(), 4);
    assert_eq!(ConstValue::Bool(true).literal_size(), 4);
    assert_eq!(ConstValue::Null.literal_size(), 4);
    // `"ab"` — two bytes plus the quotes.
    assert_eq!(ConstValue::Str("ab".to_string()).literal_size(), 4);
    // A newline emits as `\n`: two bytes, not one.
    assert_eq!(ConstValue::Str("a\nb".to_string()).literal_size(), 6);
    // A control byte emits as ``: six.
    assert_eq!(ConstValue::Str("\u{1}".to_string()).literal_size(), 8);
    // `[1,2,3]`.
    assert_eq!(
        ConstValue::Array(vec![
            ConstValue::Number(1.0),
            ConstValue::Number(2.0),
            ConstValue::Number(3.0),
        ])
        .literal_size(),
        7
    );
    // `new Set([1])` — nine for the wrapper, three for `[1]`.
    assert_eq!(
        ConstValue::Set(vec![ConstValue::Number(1.0)]).literal_size(),
        12
    );
    // `new Map([[1,2]])`.
    assert_eq!(
        ConstValue::Map(vec![(ConstValue::Number(1.0), ConstValue::Number(2.0))]).literal_size(),
        16
    );
}
