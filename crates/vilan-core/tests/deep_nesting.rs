//! The phase-1 walk is depth-BOUNDED (B138): an expression nesting past
//! `WALK_DEPTH_LIMIT` (500) levels gets a clean diagnostic, never a stack
//! overflow.
//!
//! The walk recurses once per level of syntactic nesting with the largest
//! frame in the analyzer (~36 KiB per level unoptimized, `VILAN_DEPTH_STATS`
//! measured), which is how a modest server program's analysis closed a CI
//! worker's ~2 MiB margin in the v0.36.0 incident (commit 0fb5e5f0). The
//! worker below spawns with 64 MiB ON PURPOSE — not the harness convention's
//! 256 MiB: the plant's 5000 levels cost the UNBOUNDED walk ~180 MiB
//! unoptimized and overflowed exactly this spawn before the bound existed,
//! while the bounded walk stops near 18 MiB. Growing this spawn to make a
//! failure pass again would make the pin vacuous.

use std::path::{Path, PathBuf};

use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// What one analysis on the 64 MiB worker is asked about.
struct Analysis {
    /// Whether a program came out the far side — a refusal is a diagnostic, an
    /// overflow is not.
    produced: bool,
    /// The diagnostic messages, in order.
    messages: Vec<String>,
    /// Type inferences entered during this analysis, read on the worker thread
    /// because the probe is thread-local. See [`vilan_core::analyzer::inference_entry_count`].
    inference_entries: u64,
}

/// Analyzes `source` on the 64 MiB worker the module comment explains.
fn analyze_on_64_mib(source: String) -> Analysis {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let before = vilan_core::analyzer::inference_entry_count();
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("deep.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            Analysis {
                produced: program.is_some(),
                messages: errors.into_iter().map(|error| error.msg).collect(),
                inference_entries: vilan_core::analyzer::inference_entry_count() - before,
            }
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked — the depth bound must refuse, never overflow")
}

#[test]
fn a_5000_deep_expression_is_refused_cleanly() {
    // A method chain nests the walk once per link (each call's subject is the
    // previous call), and unlike right-nested arithmetic it analyzes in
    // linear time — the plant measures depth, nothing else.
    let source = format!(
        "fun main() {{\n\tlet x = \"seed\"{};\n}}\n",
        ".trim()".repeat(5000)
    );
    let Analysis {
        produced, messages, ..
    } = analyze_on_64_mib(source);
    assert!(
        produced,
        "a too-deep expression must still produce a program (the refusal is a \
         diagnostic, not an abort)"
    );
    let refusals: Vec<&String> = messages
        .iter()
        .filter(|msg| msg.contains("nests more than 500 levels deep"))
        .collect();
    assert_eq!(
        refusals.len(),
        1,
        "the bound refuses ONCE per analysis with the steering diagnostic, \
         got: {messages:#?}"
    );
    assert!(
        refusals[0].contains("lift inner expressions into `let` bindings"),
        "the refusal must steer toward the flattening fix, got: {}",
        refusals[0]
    );
}

/// The PARSER has no depth bound (backlog B142), and it is the pipeline's
/// deepest stack consumer: `VILAN_DEPTH_STATS`'s `parse` family measures
/// `Parser::parse_atom` at ~71.8 KiB per level of source nesting unoptimized
/// (~20.3 KiB optimized) — twice the bounded phase-1 walk's frame unoptimized,
/// four times it optimized — with no limit at all.
///
/// It also runs FIRST, so it reaches the stack cliff before either analyzer
/// bound can refuse: `a_5000_deep_expression_is_refused_cleanly` above only
/// works because a METHOD CHAIN is flat to the parser and deep only to the
/// walk. Nest the source syntactically instead — 5000 parentheses re-enter
/// `parse_atom` once per level — and the file dies with no diagnostic at all,
/// which is the one outcome the depth work exists to prevent.
///
/// This asserts the behaviour B142 WILL have: the same three claims the walk's
/// bound already satisfies, on the same 64 MiB worker (which must NOT grow —
/// see the module comment).
#[test]
#[ignore = "B142: the parser has no depth bound, so nested parentheses overflow the stack before any analyzer bound can refuse"]
fn a_5000_deep_parenthesized_expression_is_refused_cleanly() {
    let source = format!(
        "fun main() {{\n\tlet x = {}1{};\n}}\n",
        "(".repeat(5000),
        ")".repeat(5000)
    );
    let Analysis {
        produced, messages, ..
    } = analyze_on_64_mib(source);
    assert!(
        produced,
        "a too-deeply-nested expression must still produce a program (the \
         refusal is a diagnostic, not an abort)"
    );
    let refusals: Vec<&String> = messages
        .iter()
        .filter(|msg| msg.contains("nests more than 500 levels deep"))
        .collect();
    assert_eq!(
        refusals.len(),
        1,
        "the parser's bound must refuse ONCE per parse with a steering \
         diagnostic, got: {messages:#?}"
    );
    assert!(
        refusals[0].contains("lift inner expressions into `let` bindings"),
        "the refusal must steer toward the flattening fix, got: {}",
        refusals[0]
    );
}

#[test]
fn realistic_nesting_is_nowhere_near_the_bound() {
    // Twenty levels is the deepest any realistic fixture measures (both
    // walkthrough entries, the std twin-parity and release-emission corpora
    // all peak at 20); the bound must be invisible from there.
    let source = format!(
        "fun main() {{\n\tlet x = \"seed\"{};\n}}\n",
        ".trim()".repeat(20)
    );
    let Analysis {
        produced, messages, ..
    } = analyze_on_64_mib(source);
    assert!(produced, "a 20-deep chain analyzes normally");
    assert!(
        messages.is_empty(),
        "no diagnostic within 25x of realistic depth, got: {messages:#?}"
    );
}

/// An `n`-function call chain in which NO function declares a return type, so
/// every link's return must be inferred from the one below it. Written
/// caller-first, which is the order that defeats the recorded-answer fast path:
/// the deepest ask arrives before anything beneath it has been recorded.
fn undeclared_return_chain(links: usize) -> String {
    let mut source = format!("fun main() {{\n\tlet x = f{}();\n}}\n", links - 1);
    for link in (1..links).rev() {
        source.push_str(&format!("\nfun f{link}() {{\n\tf{}()\n}}\n", link - 1));
    }
    source.push_str("\nfun f0() {\n\t1\n}\n");
    source
}

#[test]
fn a_too_long_return_inference_chain_is_refused_cleanly() {
    // Return inference recurses once per CALL LINK, not per level of syntactic
    // nesting, so it is a second unbounded family with its own plant (B139):
    // reading `f`'s undeclared return reads `g`'s, whose tail calls `h`. At
    // ~12.8 KiB per link unoptimized a 1200-link chain wanted ~15 MiB of stack
    // and nothing stopped it; `return_inference_stack` guards CYCLES, not
    // depth. Bounded, this peaks at 503 frames whatever the chain's length.
    let Analysis {
        produced, messages, ..
    } = analyze_on_64_mib(undeclared_return_chain(1200));
    assert!(
        produced,
        "a too-long inference chain must still produce a program (the refusal \
         is a diagnostic, not an abort)"
    );
    let refusals: Vec<&String> = messages
        .iter()
        .filter(|msg| msg.contains("needs a chain of more than 500 calls"))
        .collect();
    assert_eq!(
        refusals.len(),
        1,
        "the bound refuses ONCE per analysis, not once per link and not once \
         per constraint-resolution pass, got: {messages:#?}"
    );
    assert!(
        refusals[0].contains("declare a return type"),
        "the refusal must steer toward the annotation that ends the chain, \
         got: {}",
        refusals[0]
    );
}

#[test]
fn a_realistic_return_inference_chain_is_nowhere_near_the_bound() {
    // Corpus chains of functions that ALL decline to declare a return type peak
    // in single digits; 40 is far past realistic and far short of the bound.
    let Analysis {
        produced, messages, ..
    } = analyze_on_64_mib(undeclared_return_chain(40));
    assert!(produced, "a 40-link chain analyzes normally");
    assert!(
        messages.is_empty(),
        "no diagnostic well inside the bound, got: {messages:#?}"
    );
}

/// The same `n`-link chain as [`undeclared_return_chain`], written CALLEE-FIRST
/// (`f0` at the top, `main` at the bottom). This is the order the recorded-answer
/// fast path was built for: by the time a link is asked, every link beneath it
/// has already been inferred top-level and recorded, so the ask is answered from
/// the record instead of descending into it.
fn undeclared_return_chain_callee_first(links: usize) -> String {
    let mut source = String::from("fun f0() {\n\t1\n}\n");
    for link in 1..links {
        source.push_str(&format!("\nfun f{link}() {{\n\tf{}()\n}}\n", link - 1));
    }
    source.push_str(&format!(
        "\nfun main() {{\n\tlet x = f{}();\n}}\n",
        links - 1
    ));
    source
}

#[test]
fn a_callee_first_return_chain_flattens_instead_of_reaching_the_bound() {
    // The DEPTH bound's companion claim (B139): the bound is order-dependent
    // because the recorded-answer fast path is. Written callee-first the
    // recursion does not descend at all — the 500-link bound that a
    // caller-first chain of this length hits (the test above) is never
    // approached, because each ask is answered from the record rather than by
    // recursing (measured 502 frames / 6.26 MiB caller-first against 6 / 0.07
    // MiB callee-first).
    //
    // Non-vacuous: with the recorded-answer read disabled this chain refuses
    // with the 500-call diagnostic, exactly as the caller-first one does.
    let Analysis {
        produced, messages, ..
    } = analyze_on_64_mib(undeclared_return_chain_callee_first(1200));
    assert!(produced, "a callee-first 1200-link chain analyzes normally");
    assert!(
        messages.is_empty(),
        "a callee-first chain flattens through the recorded answer, so 1200 \
         links must not reach the 500-link bound at all, got: {messages:#?}"
    );
}

/// The chain length whose cost is the sweep's constant term — short enough to be
/// nowhere near any bound, long enough to be a real chain.
const BASELINE_LINKS: usize = 8;

/// The marginal cost the sweep allows per added link. The measured figure is 6
/// inference entries per link in both source orders; this is over 3x that, which
/// is room for an honest change to the number of times a link is visited and no
/// room at all for a curve.
const ENTRIES_PER_LINK_CEILING: u64 = 20;

/// An undeclared-return chain costs a LINE in its length, not a curve — B139's
/// TIME half, which until now was evidenced only by a prose measurement.
///
/// `inferred_return_types` already recorded each exact answer, but only a
/// read-only coercion path read it, so every link's own `FunctionReturns`
/// constraint re-derived every link beneath it: `C(n) = C(n-1) + n`, quadratic.
/// Reading the record in `infer_function_returns` is the fix.
///
/// **Why a COUNT and not a clock**, the `parse_nesting_cost` argument (B140) for
/// the analyzer's side of the pipeline: a wall ceiling generous enough to be
/// stable across machines separates linear from the 390 984-entry original, but
/// it would NOT catch a regression to merely QUADRATIC — and quadratic is
/// precisely what this bug WAS, not some distant worse case, so it is the shape
/// the pin has to catch. Comparing entries against a line in the chain's length
/// catches both.
///
/// The constant term is measured in-run rather than written down, because it is
/// std's own inference cost and std legitimately grows; only the MARGINAL cost
/// per link is a claim about this code.
fn assert_linear_in_chain_length(plant: fn(usize) -> String, label: &str) {
    // The first analysis in a process pays one-off, process-global std caching
    // — 11 898 entries against 1 850 for an identical later one — so the
    // baseline is taken after a throwaway warm-up. Without it the constant term
    // would be a one-off and the ceiling meaninglessly generous.
    let _warm_up = analyze_on_64_mib(plant(BASELINE_LINKS));
    let baseline = analyze_on_64_mib(plant(BASELINE_LINKS));
    assert!(
        baseline.produced && baseline.messages.is_empty(),
        "the {label} baseline plant must analyze cleanly, got: {:#?}",
        baseline.messages
    );
    // Every depth stays under RETURN_DEPTH_LIMIT: past the bound the chain is
    // truncated and its cost stops being the thing under measurement.
    for links in [25usize, 50, 100, 200, 400] {
        let analysis = analyze_on_64_mib(plant(links));
        assert!(
            analysis.produced && analysis.messages.is_empty(),
            "the {label} plant at {links} links must analyze cleanly, got: {:#?}",
            analysis.messages
        );
        let ceiling = baseline.inference_entries + ENTRIES_PER_LINK_CEILING * links as u64;
        assert!(
            analysis.inference_entries <= ceiling,
            "{label} at {links} links cost {} inference entries, over the linear \
             ceiling of {ceiling} — a link's return is being re-derived by the \
             links above it again (B139)",
            analysis.inference_entries
        );
    }
}

#[test]
fn a_caller_first_return_chain_costs_a_line_in_its_length() {
    // Caller-first is the order that DEFEATS the fast path on the way down: the
    // deepest ask arrives before anything beneath it has been recorded, so the
    // record is only populated on the way back up. It is the order the quadratic
    // cost was measured in, and the one that must stay linear.
    assert_linear_in_chain_length(undeclared_return_chain, "a caller-first chain");
}

#[test]
fn a_callee_first_return_chain_costs_a_line_in_its_length() {
    // The other half of "linear in BOTH source orders". Separate from the
    // caller-first case on purpose: two claims, two tests, so a failure in one
    // order still reports the other.
    assert_linear_in_chain_length(undeclared_return_chain_callee_first, "a callee-first chain");
}
