//! Where the `const` inference sweep may be called from (proposal/const-eval.md
//! §9.6) — a SOURCE-LEVEL guard, in the shape the playground's split guard
//! established (`bundle-splitting.md` §11).
//!
//! §4's tooling split is unconditional: the language server evaluates explicit
//! `const` expressions, and NEVER runs G3's inference sweep. Inference is
//! silent-fallback optimization — it produces no diagnostics and nothing an
//! editor could surface — so it belongs to the CLI's build path alone.
//!
//! Why the guard is source-level rather than behavioural. An output pin would
//! be vacuous today: `analyze_source` evaluates consts with
//! `BuildOptions::default()`, which is the DEBUG preset, so `infer_const` is
//! off there whatever the call graph looks like, and a leaked call would fold
//! nothing and be invisible. It would also be a trap of exactly the kind
//! `bundle-splitting.md` §11 describes — the day someone flips the default, or
//! threads real options into `analyze_source`, the language server would start
//! running a several-millisecond sweep on every keystroke and the wasm
//! playground would start doing release codegen, with no test to say so.
//!
//! This is the v0.23.0 lesson restated. `Instant::now()` aborts on
//! `wasm32-unknown-unknown`, the phase-timing marks ran unconditionally inside
//! `analyze()`, and it took a deploy smoke test to find out (CHANGELOG v0.23.0).
//! The sweep itself is wasm-safe — no clock, no filesystem, no environment — so
//! linking it into `vilan-core` is fine; what must not happen is the analysis
//! path *calling* it. The guard fails on the line that introduces the call,
//! which is where the decision actually gets made.

/// The sweep's entry point, spelled once. A rename that misses this file would
/// turn the guard vacuous, so it is also asserted to exist below.
const SWEEP: &str = "const_eval::infer";

/// The bare function name, which is what a same-module call would use.
const SWEEP_FUNCTION: &str = "pub fn infer";

/// `analyze_source` — the function the language server, the wasm playground,
/// and every test harness enter through — must not run the sweep.
#[test]
fn the_analysis_path_never_calls_the_inference_sweep() {
    let source = include_str!("../src/lib.rs");
    assert!(
        !source.contains(SWEEP),
        "`{SWEEP}` is a `vilan` BUILD decision (const-eval.md §4, §9.6): the \
         language server and the wasm playground both enter through \
         `analyze_source`, and inference is silent-fallback optimization with \
         nothing for an editor to surface. Call it from the CLI's \
         `compile_to_js`, not from here."
    );
}

/// The wasm playground compiles through its own path; it has no manifest to
/// declare a preset in and no reason to pay for release codegen.
#[test]
fn the_playground_never_calls_the_inference_sweep() {
    let source = include_str!("../../vilan-wasm/src/lib.rs");
    assert!(
        !source.contains(SWEEP),
        "the playground must not run the inference sweep (const-eval.md §9.6)"
    );
}

/// The language server, stated directly rather than left to follow from
/// `analyze_source`: it has other compile entry points, and §4's split is about
/// the LSP specifically.
#[test]
fn the_language_server_never_calls_the_inference_sweep() {
    for (name, source) in [
        ("main.rs", include_str!("../../vilan-lsp/src/main.rs")),
        (
            "document.rs",
            include_str!("../../vilan-lsp/src/document.rs"),
        ),
        ("publish.rs", include_str!("../../vilan-lsp/src/publish.rs")),
    ] {
        assert!(
            !source.contains(SWEEP),
            "vilan-lsp/src/{name} must not run the inference sweep: it produces \
             no diagnostics, so there is nothing for an editor to show \
             (const-eval.md §4, §9.6)"
        );
    }
}

/// The guard's own non-vacuity, in both directions: the name it greps for has
/// to be the real one, and the one place that IS allowed to call it has to be
/// calling it. Without this, deleting the sweep — or renaming it — would leave
/// three cheerfully passing tests behind.
#[test]
fn the_guarded_name_is_real_and_the_cli_is_the_one_caller() {
    assert!(
        include_str!("../src/const_eval.rs").contains(SWEEP_FUNCTION),
        "`{SWEEP_FUNCTION}` is gone from const_eval.rs — the guards above are \
         now greping for a name that does not exist"
    );
    let cli = include_str!("../../vilan-cli/src/main.rs");
    assert_eq!(
        cli.matches(SWEEP).count(),
        1,
        "the CLI must call `{SWEEP}` exactly once, on the single `compile_to_js` \
         seam every `build`/`run`/`check` round passes through"
    );
}
