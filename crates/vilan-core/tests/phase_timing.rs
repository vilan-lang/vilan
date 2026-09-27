//! `VILAN_PHASE_TIMING` is an instrument: switching it on must not change what
//! an analysis PRODUCES.
//!
//! It did. The phase marks are read inside `analyze_source`'s `catch_unwind`,
//! and on a base-cache HIT the hit path refreshed the start instant but left
//! the cached (cold) base duration in place — so the printed `load+walk`
//! subtracted a large cold duration from a small warm one, `Duration`
//! underflowed, and the panic turned the whole analysis into `None`. Every
//! analysis after the first in a process silently produced no program, which
//! is the LSP's every-keystroke path and the shape of any warm measurement.
//!
//! This is its own test binary on purpose: `phase_timing_enabled()` caches the
//! variable in a `OnceLock` on first read, so the switch has to be set before
//! any analysis in the process and must not leak into unrelated tests. One
//! test per binary also means no second thread races the `set_var`.

use std::path::{Path, PathBuf};

use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

#[test]
fn phase_timing_survives_a_warm_second_analysis() {
    // SAFETY: this binary contains exactly one test, so nothing else in the
    // process reads the environment while it is written, and it is written
    // before the first analysis (which is what caches the flag).
    unsafe { std::env::set_var("VILAN_PHASE_TIMING", "1") };

    // The switch is PUBLIC (E106): the language server adds its own phases —
    // project resolution, the editor tables, a shared module's further legs,
    // all of them outside `analyze` and so invisible to the line below — to the
    // same output under this one variable. A front end that could not ask would
    // need a second switch, and two half-pictures is what the split exists to
    // avoid. Read here, before any analysis, because the answer is cached.
    assert!(
        vilan_core::phase_timing_enabled(),
        "the switch a front end reads must answer for the variable that is set"
    );

    let source = r#"
        import std::io::print;
        fun main() { print("warm"); }
        main();
        "#;
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            // The first analysis fills the base cache; the second HITS it, and
            // the hit is the path that used to underflow. Two distinct leaks of
            // the same text, because `analyze_source` wants `&'static str`.
            for round in 0..2 {
                let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    Path::new("."),
                    Path::new("warm.vl"),
                    Some(Platform::default()),
                    &Workspace::default(),
                );
                let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
                assert!(
                    messages.is_empty(),
                    "round {round}: expected a clean analysis, got: {messages:#?}"
                );
                assert!(
                    program.is_some(),
                    "round {round}: the phase instrument panicked the analysis away — \
                     with `VILAN_PHASE_TIMING` set, a warm analysis must still produce a program"
                );
            }
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}
