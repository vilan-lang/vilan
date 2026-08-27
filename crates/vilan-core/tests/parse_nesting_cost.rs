//! Nested expressions cost the parser a LINE in their nesting depth, not a
//! curve (backlog B140).
//!
//! `parse_assignment` is tried before the operator tower and discovers whether
//! an assignment operator follows a place by parsing the whole precedence chain
//! speculatively — then throwing it away when none does. The tower then parses
//! the same text again, so every expression that re-enters `parse_expression`
//! inside a bracket paid for its own subtree TWICE, once per level:
//! `C(n) = 2·C(n-1)`. `(1 + (1 + …))` at 20 levels cost 9.0s in the parser
//! alone and 60 levels did not finish; nested array literals were the same
//! class. The fix is `assignment_reachable`, a per-position table that skips the
//! speculation when no assignment operator can follow.
//!
//! **Why a COUNT and not a clock.** The bug was diagnosed as an ANALYZER
//! problem for two days because wall-clock does not say who spent it — the
//! analyzer's own phase clocks stayed flat at 200ms while `analyze_source` took
//! 8.4s. A count says exactly what grew. It is also the stricter pin: a
//! generous wall ceiling that separates linear from exponential (at these
//! depths, by a factor with eleven digits) would not notice a regression to
//! merely QUADRATIC, which is a live risk here — the natural cheap fix for this
//! bug is a per-attempt lookahead scan, and that is quadratic. Asserting the
//! count against a LINE in the depth catches both. `atom_parse_count` is the
//! `buffer_parse_count` probe shape (E83), for the same reason.
//!
//! The worker stack is the deep-nesting convention, not a margin claim: the
//! parser is not depth-bounded (B139's residual), and these depths are chosen
//! to measure cost, not to probe the depth cliff.

use vilan_core::parsing;

/// Atoms entered while parsing `source`, which must parse cleanly.
fn atoms_to_parse(source: String) -> u64 {
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let before = parsing::atom_parse_count();
    let (tree, errors) = parsing::parse(leaked);
    assert!(
        tree.is_some() && errors.is_empty(),
        "the plant must parse cleanly, got {errors:#?}"
    );
    parsing::atom_parse_count() - before
}

/// `(1 + (1 + … 1 …))`, `depth` levels of right-nested parenthesized addition.
fn nested_arithmetic(depth: usize) -> String {
    let mut body = String::from("1");
    for _ in 0..depth {
        body = format!("(1 + {body})");
    }
    format!("fun main() {{\n\tlet x = {body};\n}}\n")
}

/// `[[[… [1] …]]]`, `depth` levels of nested list literals — the same class.
fn nested_lists(depth: usize) -> String {
    let mut body = String::from("[1]");
    for _ in 0..depth {
        body = format!("[{body}]");
    }
    format!("fun main() {{\n\tlet x = {body};\n}}\n")
}

/// The measured cost is `2·depth + 2` atoms for the arithmetic plant and
/// `depth + 3` for the list plant. The ceiling is that line with room to spare
/// — generous enough that an honest grammar change may move the constant, tight
/// enough that no super-linear cost fits under it: at the largest depth here a
/// quadratic parser would want ~65 000 atoms against a ceiling of 2 112, and the
/// exponential one this pins wanted 2^256.
fn assert_linear_in_depth(plant: fn(usize) -> String, label: &str) {
    for depth in [16usize, 32, 64, 128, 256] {
        let atoms = atoms_to_parse(plant(depth));
        let ceiling = 8 * depth as u64 + 64;
        assert!(
            atoms <= ceiling,
            "{label} at {depth} levels cost {atoms} atoms, over the linear \
             ceiling of {ceiling} — nested expressions are being parsed more \
             than a constant number of times per level again (B140)"
        );
    }
}

#[test]
fn nested_expressions_cost_a_line_in_their_depth() {
    // The parser is not depth-bounded (B139), so the plants run on the
    // deep-nesting worker rather than libtest's ~2 MiB thread.
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            assert_linear_in_depth(nested_arithmetic, "nested arithmetic");
            assert_linear_in_depth(nested_lists, "nested list literals");
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}
