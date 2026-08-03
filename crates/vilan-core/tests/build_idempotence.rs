//! The S2 pin (proposal/analysis-reuse.md §6.3): a second `build()` over the
//! drained resolution queues is observationally neutral — identical
//! diagnostics, warnings, and emitted JS. This is the contract S3's
//! re-entrant builds over a frozen std base stand on. Before S2, `build()`
//! CLONED `prepped_imports`/`prepped_locals`/`prepped_type_locals` and both
//! accessor queues, so a second run re-resolved everything: `reference_count`
//! double-incremented (observable through copy-elision's used-exactly-once
//! test, which changes emitted JS) and failing imports reported twice.
//!
//! `set_build_twice` is process-global, so the tests serialize on one lock.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

static OVERRIDE_LOCK: Mutex<()> = Mutex::new(());

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// One analysis + transform on a big-stack worker; the triple the pin
/// compares between single- and double-build runs.
fn observe(source: &'static str) -> (String, String, Option<String>) {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &std_spec(),
                Path::new("."),
                Path::new("idempotence.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let diagnostics = format!("{errors:?}");
            let warnings = program
                .as_ref()
                .map(|program| format!("{:?}", program.warnings))
                .unwrap_or_default();
            let javascript = match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).ok()
                }
                _ => None,
            };
            (diagnostics, warnings, javascript)
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// The battery: each program exercises a resolution path whose re-run used
/// to corrupt state. Compared single-build vs double-build.
const BATTERY: &[(&str, &str)] = &[
    (
        // Copy-elision reads `reference_count == 1`: a binding aliased
        // exactly once elides the copy in the emitted JS. A re-resolved
        // local counts twice and the copy reappears — a byte-level JS diff.
        "use-once alias elision",
        r#"
import std::print;
fun make(): List<i32> {
    mut items: List<i32> = List::new();
    items.push(1);
    items
}
fun main() {
    let first = make();
    mut second = first;
    second.push(3);
    print(second);
}
"#,
    ),
    (
        // A failing import: re-resolution used to report the error twice.
        "failing import",
        r#"
import std::this_module_does_not_exist;
fun main() {}
"#,
    ),
    (
        // Nested item imports + static accessors (Shared::new) — the
        // accessor queues also re-incremented member reference counts.
        "item imports and static accessors",
        r#"
import std::print;
import std::shared::Shared;
fun main() {
    let cell = Shared::new(41);
    cell.write() = cell.read() + 1;
    print(cell.read());
}
"#,
    ),
    (
        // An unused import (warning path) plus a used one: warning output
        // must be identical, not doubled or flipped.
        "unused import warning",
        r#"
import std::print;
import std::time::sleep;
fun main() { print(1); }
"#,
    ),
];

#[test]
fn a_second_build_changes_nothing_observable() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (label, source) in BATTERY {
        vilan_core::analyzer::set_build_twice(false);
        let single = observe(source);
        vilan_core::analyzer::set_build_twice(true);
        let doubled = observe(source);
        vilan_core::analyzer::set_build_twice(false);
        assert_eq!(
            single.0, doubled.0,
            "{label}: diagnostics differ under a second build()"
        );
        assert_eq!(
            single.1, doubled.1,
            "{label}: warnings differ under a second build()"
        );
        assert_eq!(
            single.2.is_some(),
            doubled.2.is_some(),
            "{label}: compile cleanliness differs under a second build()"
        );
        assert_eq!(
            single.2, doubled.2,
            "{label}: emitted JS differs under a second build()"
        );
    }
}

/// S3b's kernel, landed (analysis-reuse.md §6.8): under a two-phase build
/// (std resolved before the entry walks), an immediate-chained generic
/// method call stalled — the constraint fixpoint declared quiescence when a
/// backstop pass resolved nothing and woke nothing, but a deferred attempt
/// can WRITE types (a method call types its closure argument's parameters,
/// then defers at the incomplete-bindings guard) without either signal
/// firing. Std's unrelated constraint churn masked the early exit
/// monolithically by granting extra rounds; the two-phase probe unmasked
/// it. The exit now also requires an untouched type map across the
/// backstop retry. This test was `#[ignore]`d red while the kernel was
/// open; it is the standing two-phase inference gate now.
#[test]
fn two_phase_build_resolves_chained_generic_calls() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::set_early_std_build(true);
    let observation = observe(
        r#"
import std::print;
struct Point { x: i32, name: str }
fun main() {
    let points = [Point { x = 1, name = "abc" }];
    let lens = points.map(|p| p.name).map(|s| s.len());
    print(lens[0]);
}
"#,
    );
    vilan_core::analyzer::set_early_std_build(false);
    assert_eq!(
        observation.0, "[]",
        "the chained generic call must infer under a two-phase build"
    );
    assert!(observation.2.is_some(), "and emit JS");
}
