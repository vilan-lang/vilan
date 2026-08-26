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

/// (program, diagnostics) for `source`, analyzed on the 64 MiB worker the
/// module comment explains.
fn analyze_on_64_mib(source: String) -> (bool, Vec<String>) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("deep.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            (
                program.is_some(),
                errors.into_iter().map(|error| error.msg).collect(),
            )
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
    let (produced, messages) = analyze_on_64_mib(source);
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

#[test]
fn realistic_nesting_is_nowhere_near_the_bound() {
    // Twenty levels is the deepest any realistic fixture measures (both
    // walkthrough entries, the std twin-parity and release-emission corpora
    // all peak at 20); the bound must be invisible from there.
    let source = format!(
        "fun main() {{\n\tlet x = \"seed\"{};\n}}\n",
        ".trim()".repeat(20)
    );
    let (produced, messages) = analyze_on_64_mib(source);
    assert!(produced, "a 20-deep chain analyzes normally");
    assert!(
        messages.is_empty(),
        "no diagnostic within 25x of realistic depth, got: {messages:#?}"
    );
}
