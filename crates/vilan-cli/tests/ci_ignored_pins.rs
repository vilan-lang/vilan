//! The weekly ignored-pins leg (backlog N27), held to the decisions that make
//! it worth having.
//!
//! Nothing else in the tree runs `.github/workflows/ignored-pins.yml` — it
//! fires on a cron, once a week, and a leg that has quietly stopped excluding
//! the 247-second pin, stopped being non-blocking, or started using nextest's
//! deprecated flag spelling would first be noticed by whoever waits on it. So
//! these pins read the workflow and hold the four decisions in it, plus the one
//! fact outside it the leg depends on: that the test it filters by name is
//! still a test.

use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn workflow() -> String {
    std::fs::read_to_string(repository_root().join(".github/workflows/ignored-pins.yml"))
        .expect("the ignored-pins workflow is committed")
}

/// The workflow with every comment line dropped — YAML's and the shell's alike.
/// This file's subject is what the leg DOES, and the workflow explains itself at
/// length: a `contains` over the whole text is satisfied by prose describing the
/// very decision that was deleted, which is a vacuous pin wearing a green tick.
fn workflow_code() -> String {
    workflow()
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The pin the leg excludes by name — 247 s of measurement whose `#[ignore]`
/// reason is cost, not a bug, so running it buys the leg nothing.
const EXCLUDED_PIN: &str = "perf_baseline_lsp_edit_latency";

// It runs the ignored set, and it runs it with the spelling nextest still
// documents. `ignored-only` is a deprecated alias that works today and is
// exactly the kind of thing a future nextest removes; `only` is the value its
// `--help` lists.
#[test]
fn the_leg_runs_the_ignored_set_with_nextests_current_flag_spelling() {
    let workflow = workflow_code();
    assert!(
        workflow.contains("--run-ignored only"),
        "the leg must run the ignored set: {workflow}"
    );
    assert!(
        !workflow.contains("--run-ignored ignored-only"),
        "`ignored-only` is nextest's DEPRECATED alias — the spelling is `only`"
    );
    assert!(
        workflow.contains("cargo nextest run --workspace"),
        "the leg runs the whole workspace's ignored set"
    );
}

// The exclusion, and the fact it depends on: a filter naming a test that no
// longer exists excludes nothing and costs the leg a quarter of its budget in
// silence.
#[test]
fn the_leg_excludes_the_expensive_pin_and_that_pin_still_exists() {
    assert!(
        workflow_code().contains(&format!("not test({EXCLUDED_PIN})")),
        "the leg must filter {EXCLUDED_PIN} out of the ignored set"
    );

    let document =
        std::fs::read_to_string(repository_root().join("crates/vilan-lsp/src/document.rs"))
            .expect("read the lsp document module");
    assert!(
        document.contains(&format!("fn {EXCLUDED_PIN}(")),
        "the leg filters {EXCLUDED_PIN} by name, and no test by that name exists any more — \
         the filter now excludes nothing"
    );
}

// Non-blocking, by construction. The bug pins in the ignored set are expected
// RED; a gate here would be red by design, and the signal it exists for is the
// opposite one (a pin that now passes).
#[test]
fn the_leg_is_advisory_and_reports_a_pin_that_now_passes() {
    let workflow = workflow_code();
    // The KEY, not the word: the header comment says `continue-on-error: true`
    // in prose, so a `contains` over the whole file passes on a workflow that
    // only talks about being advisory.
    assert!(
        workflow
            .lines()
            .any(|line| line.trim() == "continue-on-error: true"),
        "the leg must not block: its bug pins are expected red"
    );
    assert!(
        workflow.contains("GITHUB_STEP_SUMMARY"),
        "the leg's product is a job summary — that is the whole deliverable"
    );
    assert!(
        workflow.contains("now PASSES"),
        "the summary must call out a pin that PASSES: an expired `#[ignore]` reason is \
         the finding this leg exists to make"
    );
    assert!(
        workflow.contains("cron:"),
        "the leg is periodic, not per-push"
    );
}

// …and it is out of the required check's way. `ci / check` is the required
// status context (process.md §2.5): a required check that can be skipped, or
// that waits on an advisory job, strands a PR at "Expected" forever.
#[test]
fn the_required_check_does_not_wait_on_the_advisory_leg() {
    let ci = std::fs::read_to_string(repository_root().join(".github/workflows/ci.yml"))
        .expect("the ci workflow is committed");
    let needs = ci
        .lines()
        .find(|line| line.trim_start().starts_with("needs: [changes"))
        .expect("`check` declares its needed jobs on one line");
    assert_eq!(
        needs.trim(),
        "needs: [changes, test, wasm, fmt]",
        "`check` gained or lost a needed job — if the advisory ignored-pins leg is in \
         there, a weekly report can now block a PR"
    );
    assert!(
        !ci.contains("ignored-pins"),
        "the advisory leg lives in its own workflow so `ci / check` cannot wait on it"
    );
}
