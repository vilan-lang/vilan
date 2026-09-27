//! How many macro WORLDS an analysis compiled is a number the compiler can
//! answer about itself (tracker M33).
//!
//! A macro world is a nested analysis — the blanked copy of one macro-defining
//! file, compiled against `macro_std` (macro-engine.md §3). A cold `vilan check`
//! of kolt's client compiles four of them, 18.2% of that entry's instructions,
//! and nothing said so: the world's own phase line is suppressed (its numbers
//! inside the outer analysis's would read as noise), so the four folded into
//! the outer entry's `load+walk` and left only four unheaded `post-passes`
//! lines behind them.
//!
//! `macro_worlds_compiled()` is the count behind the `macro-worlds` phase row,
//! readable without parsing stderr — the `bindable_set_cost` probe shape (M30),
//! and for the same reason: a claim about how often the compiler does something
//! should be assertable as a number, not inferred from a wall clock.
//!
//! **The property pinned here is that the count tracks COMPILES, not
//! dispatches.** Every derive in a program dispatches into a world; a world is
//! *compiled* once per distinct macro-definition set and cached from then on
//! (§6), so the second analysis in a process must add nothing. That is the
//! claim M33's cache half rests on — its pin is the same count reading zero
//! across a process boundary — and it is worth having on its own, because the
//! in-process cache is what makes the disk cache a layer rather than a rewrite.
//!
//! Its own test binary, like `phase_timing.rs` beside it: the world cache is
//! process-global, and a second test analyzing anything in this process would
//! warm it under the cold assertion below.

use std::path::{Path, PathBuf};

use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// A program whose `[derive]` sends the expander into std's derive world — the
/// smallest thing that compiles a world at all.
const DERIVING: &str = "[derive(Debug)]\nstruct Point { x: i32, y: i32 }\n\n\
     fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tlet shown = p.debug();\n}\n";

/// The same program with the derive removed: the control that makes the count
/// above a measurement rather than a constant.
const PLAIN: &str = "struct Point { x: i32, y: i32 }\n\n\
     fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tlet moved = p.x;\n}\n";

fn analyze(source: &str, name: &str) {
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    let (program, errors) = analyze_source(
        leaked,
        &std_spec(),
        Path::new("."),
        Path::new(name),
        Some(Platform::default()),
        &Workspace::default(),
    );
    let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
    assert!(
        messages.is_empty(),
        "{name}: expected a clean analysis, got: {messages:#?}"
    );
    assert!(program.is_some(), "{name}: expected a program");
}

#[test]
fn the_world_census_counts_compiles_and_a_warm_analysis_compiles_none() {
    // The deep-nesting convention: `analyze_source` recurses, and a macro world
    // recurses inside it.
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            // A program with no macro uses compiles no worlds — the control
            // first, while the process is genuinely cold, so a nonzero reading
            // here would mean the count is measuring something else.
            analyze(PLAIN, "plain.vl");
            assert_eq!(
                vilan_core::macro_worlds_compiled(),
                0,
                "a program that dispatches no macro must compile no macro world"
            );

            // Cold: the derive's world is compiled.
            analyze(DERIVING, "deriving.vl");
            let cold = vilan_core::macro_worlds_compiled();
            assert!(
                cold >= 1,
                "a `[derive]` must compile at least one macro world, got {cold}"
            );

            // Warm: the same definition set is served from the process cache,
            // so the second analysis compiles nothing. This is the claim the
            // on-disk table generalizes across processes.
            analyze(DERIVING, "deriving-again.vl");
            assert_eq!(
                vilan_core::macro_worlds_compiled(),
                0,
                "the second analysis of the same derive re-compiled a world the \
                 process cache already holds"
            );

            // Cleared: the cold path is reachable again, which is the only
            // thing that makes the warm reading above a claim about the CACHE
            // rather than about the second analysis. `macro_world_cache_clear`
            // drops the in-memory EXPANSION table along with the compiled
            // worlds, and it has to: the expansion key is reachable without the
            // world (M33), so a surviving expansion would answer before the
            // world was ever asked for and this analysis would compile nothing
            // — an emptier cache producing a smaller number, which is the
            // opposite of what the clear is for.
            vilan_core::macro_world_cache_clear();
            vilan_core::analyzer::base_cache_clear();
            analyze(DERIVING, "deriving-cold.vl");
            assert_eq!(
                vilan_core::macro_worlds_compiled(),
                cold,
                "with the world cache cleared the analysis must compile the same \
                 worlds it compiled the first time"
            );
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}
