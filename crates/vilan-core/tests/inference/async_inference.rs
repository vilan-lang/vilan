//! The async inference (`async_infer.rs`): the implicit-await fixpoint and the
//! adaptation worklist (async-polymorphism.md A.1). The diagnostics it reports
//! are pinned where their subject lives (`bounds.rs`, `generics.rs`, ...);
//! these pins own the pass's SHAPE — what it computes, and how often.

use super::support::*;

/// `(passes, worklist rounds, decisions passes)` one analysis of `source` ran
/// (a pass per analysis, the macro worlds it builds included).
fn adaptation_counts(source: &str) -> (usize, usize, usize) {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let source: &'static str = Box::leak(source.into_boxed_str());
            vilan_core::async_infer::reset_adaptation_counts();
            let (program, errors) = analyze_source(
                source,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            drop(program);
            vilan_core::async_infer::adaptation_counts()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// M138: the worklist re-runs while a round grows the async set, but the
/// DECISIONS pass — each instance's emission data and diagnostics, which reads
/// the set and moves nothing — runs once, over the last round. `caller` passes
/// an awaiting closure to `apply`, whose adapted instance is async, so the
/// first round flips `caller` itself async and a second round must run; the
/// first round's decisions were computed and thrown away.
#[test]
fn m138_the_adaptation_decisions_run_once_however_many_rounds() {
    let (passes, rounds, decisions) = adaptation_counts(
        r#"
        import std::io::print;
        import std::display::Display;
        import std::time::sleep;

        fun apply(f: || i32): i32 {
            f()
        }

        fun caller(): i32 {
            apply(|| {
                sleep(1);
                3
            })
        }

        fun main() {
            print(caller().to_string());
        }
        "#,
    );
    assert!(
        rounds > passes,
        "the probe must need a second worklist round, or the count below proves nothing: \
         {rounds} round(s) over {passes} pass(es)"
    );
    assert_eq!(
        decisions, passes,
        "{rounds} worklist rounds over {passes} pass(es) ran {decisions} decisions passes: the \
         decisions read each pass's last round only (M138)"
    );
}
