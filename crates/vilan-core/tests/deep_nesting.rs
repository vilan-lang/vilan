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
//!
//! The file grew past that one bound, because the recursive families that can
//! reach the stack cliff share the worker and the argument. It now pins, in
//! order: the phase-1 walk's bound (B138); the return-inference chain's bound,
//! its COST — a line in the chain's length, in both source orders — and the
//! flattening the recorded answer buys when the callee is defined first (all
//! B139); and the PARSER's own bound, one pin per door into a nested grammar
//! (B142). The cost pins live here rather than in a file of their own so that
//! the chain plants have ONE definition: a plant copied into a second file is a
//! plant that drifts.

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

/// The PARSER is bounded too (B142), and it is the pipeline's deepest stack
/// consumer: `VILAN_DEPTH_STATS`'s `parse` family measured it at ~71.8 KiB per
/// level of source nesting unoptimized (~20.3 KiB optimized) — twice the
/// bounded phase-1 walk's frame unoptimized, four times it optimized.
///
/// It also runs FIRST, so before the bound it reached the stack cliff before
/// either analyzer bound could refuse: `a_5000_deep_expression_is_refused_cleanly`
/// above only works because a METHOD CHAIN is flat to the parser and deep only
/// to the walk. Nest the source syntactically instead — 5000 parentheses
/// re-enter the expression grammar once per level — and the file used to die
/// with no diagnostic at all, which is the one outcome the depth work exists to
/// prevent.
///
/// This is the END-TO-END leg: the same three claims the walk's bound satisfies,
/// asserted through `analyze_source` so the parser's refusal is shown to survive
/// the whole pipeline and arrive as a diagnostic. `every_nesting_door_is_refused_cleanly`
/// is the exhaustive per-door leg, and parses directly. Both share the 64 MiB
/// worker, which must NOT grow — see the module comment; the bounded parse of
/// this very plant measures 35.2 MiB unoptimized, so the margin is real but not
/// large.
#[test]
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

// ---------------------------------------------------------------------------
// The parser's own bound (B142) — one pin per DOOR.
// ---------------------------------------------------------------------------

/// Parses `source` on the same 64 MiB worker, WITHOUT analyzing it. The door
/// table below is a claim about the parser alone, and parsing it directly keeps
/// the plants off std's analysis cost — twenty-one 5000-level plants through
/// `analyze_source` would dominate this binary's runtime for no added claim.
/// `a_5000_deep_parenthesized_expression_is_refused_cleanly` is the end-to-end
/// leg that proves the refusal survives the whole pipeline.
fn parse_on_64_mib(source: String) -> (bool, Vec<String>) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (tree, errors) = vilan_core::parsing::parse(leaked);
            (
                tree.is_some(),
                errors.iter().map(vilan_core::parsing::render).collect(),
            )
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked — the depth bound must refuse, never overflow")
}

/// Every door by which source can nest one level deeper, planted `levels` deep.
///
/// **Why a table and not one bound in `parse_atom`.** The instrument was first
/// hung on `parse_atom`, and a bound there would have looked right: the
/// bracketed forms all re-enter it. It would have covered the first four rows
/// here and nothing else. The block-bearing forms and the prefixes reach a
/// nested expression through `parse_secondary` without touching an atom; and
/// past the expression grammar there are five more recursive grammars — types,
/// binders/patterns, items, import paths and elements — each a closed cycle
/// that reaches no expression rule at all. Measured before the bound existed,
/// `fun a() { fun a() { .. } }` at 5000 levels overflowed this very worker with
/// no diagnostic, which is the one outcome the depth work exists to prevent.
///
/// So the plants live in a table: a door that stops being covered is a row that
/// goes red, and a door added to the grammar later is a row somebody has to
/// think about adding. Every row was confirmed to overflow or hang WITHOUT the
/// bound before being written down here.
fn nesting_doors(levels: usize) -> Vec<(&'static str, String)> {
    let n = levels;
    vec![
        // The expression grammar's bracketed forms — the four `parse_atom`
        // would have covered.
        (
            "parenthesis",
            format!(
                "fun main() {{\n\tlet x = {}1{};\n}}\n",
                "(".repeat(n),
                ")".repeat(n)
            ),
        ),
        (
            "array literal",
            format!(
                "fun main() {{\n\tlet x = {}1{};\n}}\n",
                "[".repeat(n),
                "]".repeat(n)
            ),
        ),
        (
            "call arguments",
            format!(
                "fun main() {{\n\tlet x = f{}1{};\n}}\n",
                "(f".repeat(n),
                ")".repeat(n)
            ),
        ),
        (
            "index",
            format!(
                "fun main() {{\n\tlet x = a{}1{};\n}}\n",
                "[a".repeat(n),
                "]".repeat(n)
            ),
        ),
        // Reached through `parse_secondary` without an atom.
        (
            "block",
            format!(
                "fun main() {{\n\tlet x = {}1{};\n}}\n",
                "{ ".repeat(n),
                "; }".repeat(n)
            ),
        ),
        (
            "closure body",
            format!("fun main() {{\n\tlet x = {}1;\n}}\n", "|| ".repeat(n)),
        ),
        (
            "struct literal",
            format!(
                "fun main() {{\n\tlet x = {}1{};\n}}\n",
                "S { f = ".repeat(n),
                " }".repeat(n)
            ),
        ),
        // The prefixes, which recurse into themselves.
        (
            "unary prefix",
            format!("fun main() {{\n\tlet x = {}1;\n}}\n", "!".repeat(n)),
        ),
        (
            "`const` prefix",
            format!("fun main() {{\n\tlet x = {}1;\n}}\n", "const ".repeat(n)),
        ),
        // The type grammar — a closed cycle that reaches no expression rule.
        (
            "reference type",
            format!("fun f(a: {}i64) {{\n\tvoid\n}}\n", "& ".repeat(n)),
        ),
        (
            "array type",
            format!(
                "fun f(a: {}i64{}) {{\n\tvoid\n}}\n",
                "[".repeat(n),
                "; 1]".repeat(n)
            ),
        ),
        (
            "tuple type",
            format!(
                "fun f(a: {}i64{}) {{\n\tvoid\n}}\n",
                "(".repeat(n),
                ")".repeat(n)
            ),
        ),
        (
            "generic argument",
            format!(
                "fun f(a: {}i64{}) {{\n\tvoid\n}}\n",
                "L<".repeat(n),
                ">".repeat(n)
            ),
        ),
        // Binders and patterns — the second closed cycle.
        (
            "array binder",
            format!(
                "fun main() {{\n\tlet {}a{} = void;\n}}\n",
                "[".repeat(n),
                "]".repeat(n)
            ),
        ),
        (
            "match pattern",
            format!(
                "fun main() {{\n\tmatch x {{\n\t\t{}a{} => 1,\n\t}}\n}}\n",
                "S(".repeat(n),
                ")".repeat(n)
            ),
        ),
        // Items — the cycle that closes through `parse_statement`, and the one
        // that actually overflowed this worker.
        (
            "nested `fun`",
            format!("{}\n{}\n", "fun a() {".repeat(n), "}".repeat(n)),
        ),
        (
            "nested `mod`",
            format!("{}\n{}\n", "mod a {".repeat(n), "}".repeat(n)),
        ),
        (
            "`export` chain",
            format!("{}fun f() {{\n\tvoid\n}}\n", "export ".repeat(n)),
        ),
        // Import paths and elements.
        ("import path", format!("use {}a;\n", "a::".repeat(n))),
        (
            "import set",
            format!("use {}a{};\n", "a::{".repeat(n), "}".repeat(n)),
        ),
        (
            "element",
            format!(
                "fun main() {{\n\tlet x = {}{};\n}}\n",
                "<a>".repeat(n),
                "</a>".repeat(n)
            ),
        ),
    ]
}

#[test]
fn every_nesting_door_is_refused_cleanly() {
    for (door, source) in nesting_doors(5000) {
        let (produced, messages) = parse_on_64_mib(source);
        assert!(
            produced,
            "{door}: a too-deeply-nested source must still produce a tree (the \
             refusal is a diagnostic, not an abort)"
        );
        let refusals: Vec<&String> = messages
            .iter()
            .filter(|message| message.contains("nests more than 500 levels deep"))
            .collect();
        assert_eq!(
            refusals.len(),
            1,
            "{door}: the bound must refuse ONCE per parse — 5000 levels past a \
             500-level bound would otherwise report 4500 times — got: {messages:#?}"
        );
    }
}

/// The bound must be invisible from real code, and the margin has to be read
/// against THIS counter rather than the instrument's earlier placement: types,
/// patterns, items, import paths and elements all draw on the same counter now,
/// so one level of source nesting can spend more than one level of it.
///
/// Swept over all 211 compilable corpus entries, `VILAN_DEPTH_STATS`'s `parse`
/// family peaks at 23 with a median of 14. Fifty is past every corpus file and
/// far short of the bound.
#[test]
fn realistic_parse_nesting_is_nowhere_near_the_bound() {
    for (door, source) in nesting_doors(50) {
        let (produced, messages) = parse_on_64_mib(source);
        assert!(produced, "{door}: 50 levels must parse normally");
        let refusals: Vec<&String> = messages
            .iter()
            .filter(|message| message.contains("nests more than 500 levels deep"))
            .collect();
        assert!(
            refusals.is_empty(),
            "{door}: 50 levels is past every corpus file and must be nowhere \
             near the bound, got: {messages:#?}"
        );
    }
}

/// The FORMATTER parses in its own mode (`parse_preserving_groups`, which
/// records every `(…)` as a node instead of dissolving it), and that mode is a
/// second entry into the same `parse_with` — so it must carry the same bound.
/// Pinned separately because it is a separate public entry point: a bound
/// wired into `parse` alone would leave `vilan fmt` overflowing on a file
/// `vilan build` refuses cleanly.
#[test]
fn the_formatters_parse_mode_is_bounded_too() {
    let source = format!(
        "fun main() {{\n\tlet x = {}1{};\n}}\n",
        "(".repeat(5000),
        ")".repeat(5000)
    );
    let (produced, messages) = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (tree, errors) = vilan_core::parsing::parse_preserving_groups(leaked);
            (
                tree.is_some(),
                errors
                    .iter()
                    .map(vilan_core::parsing::render)
                    .collect::<Vec<_>>(),
            )
        })
        .expect("spawn worker")
        .join()
        .expect("the formatter's parse mode must refuse, never overflow");
    assert!(produced, "group-preserving mode must still produce a tree");
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains("nests more than 500 levels deep"))
            .count(),
        1,
        "group-preserving mode must carry the same bound, got: {messages:#?}"
    );
}
