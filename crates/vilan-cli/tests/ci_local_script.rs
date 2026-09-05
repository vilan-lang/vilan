//! `scripts/ci-local.sh` and `.github/workflows/ci.yml` are ONE list of gate
//! commands (tracker L19).
//!
//! The workflow used to carry every gate command inline, which made "run CI
//! locally" a thing you did by reading a YAML file and retyping six commands —
//! and made the two drift by construction, because the retyped version is a
//! copy and a copy is only ever right on the day it is made. The script now
//! holds the commands and the workflow CALLS it, one leg per job. That is only
//! an improvement while the calling is complete: a job that goes back to
//! running `cargo …` inline is the drift again, wearing a different shape.
//!
//! # What this file verifies
//!
//! 1. **Every leg job runs the script and nothing else.** Its `run:` steps are
//!    `scripts/ci-local.sh <leg>` invocations, for the legs [`LEG_JOBS`] says
//!    that job owns.
//! 2. **The script's leg list IS the job list.** Read out of the script's own
//!    `LEGS=` line and held against ci.yml's jobs in both directions, so a leg
//!    added to one side and not the other reds here rather than in six months.
//! 3. **The local-only legs stay local-only.** `windows` is a `cargo check`
//!    cross-check standing in for a suite this box cannot run; if it ever
//!    appears in a workflow it is being passed off as the real thing.
//! 4. **Every declared leg has a function.** The script checks this at run time
//!    too; this is the half that fails without anybody running it.
//!
//! # What this file does NOT verify
//!
//! That the legs are the right legs, or that the commands are correct.
//! `release_gate.rs` owns the second question — it reads the CI side's commands
//! out of the script now, because the script is where they live.
//!
//! unix-only, like `release_scripts.rs` and `brew_formula.rs`: the subject is a
//! POSIX shell script and the windows leg of CI has no shell to run it with.
#![cfg(unix)]

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn script() -> String {
    let path = repository_root().join("scripts/ci-local.sh");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("ci-local.sh: {error}"))
}

fn workflow() -> String {
    let path = repository_root().join(".github/workflows/ci.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("ci.yml: {error}"))
}

/// Which ci.yml job runs each leg, and the whole reason this is a mapping and
/// not an equality: `doctest` is a leg of its own — nextest does not run
/// doc-tests, and once the suite is sharded a doc-test step on every shard runs
/// them more than once — but it is not a job of its own, because standing a
/// second runner up to compile the workspace for an empty doc-test set would
/// cost more than the leg does.
const LEG_JOBS: &[(&str, &str)] = &[
    ("fmt", "fmt"),
    ("vilan-fmt", "vilan-fmt"),
    ("clippy", "clippy"),
    ("test", "test"),
    ("doctest", "test"),
    ("audit", "audit"),
    ("wasm", "wasm"),
];

/// Legs with deliberately no job. See the script's own header for why the
/// windows cross-check is one.
const LOCAL_ONLY: &[&str] = &["windows"];

/// ci.yml jobs that are not legs: the path filter and the aggregate that IS the
/// required check. Neither runs a gate command, so neither calls the script.
const META_JOBS: &[&str] = &["changes", "check"];

/// The legs the script declares, in its own order.
fn declared_legs() -> Vec<String> {
    let line = script()
        .lines()
        .find(|line| line.starts_with("LEGS="))
        .expect("ci-local.sh must declare its legs on one `LEGS=` line — this file reads it")
        .to_string();
    line.trim_start_matches("LEGS=")
        .trim_matches('"')
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// ci.yml's job keys: a line at exactly two spaces of indent ending in `:`.
fn workflow_jobs(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| *line != "jobs:")
        .filter(|line| {
            line.starts_with("  ")
                && !line.chars().nth(2).is_some_and(char::is_whitespace)
                && line.trim_end().ends_with(':')
                && !line.trim_start().starts_with('#')
        })
        .map(|line| line.trim().trim_end_matches(':').to_string())
        .collect()
}

/// One job's body, by its key: everything from its header line to the next line
/// at job depth. (`release_gate.rs` reads release.yml the same way.)
fn job_body(text: &str, job: &str) -> String {
    let at_job_depth = |line: &str| {
        line.starts_with("  ") && !line.chars().nth(2).is_some_and(char::is_whitespace)
    };
    let mut lines = text.lines().skip_while(|line| *line != format!("  {job}:"));
    let head = lines
        .next()
        .unwrap_or_else(|| panic!("no `{job}:` job at job depth in ci.yml"));
    std::iter::once(head)
        .chain(lines.take_while(|line| !at_job_depth(line)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `run:` commands in `text`, one-liners and the first line of a block
/// alike — enough to tell "calls the script" from "runs cargo".
fn run_commands(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim_start().strip_prefix("run: "))
        .map(|command| command.trim().to_string())
        .collect()
}

#[test]
fn every_leg_job_runs_the_script_for_the_legs_it_owns() {
    let workflow = workflow();
    for (leg, job) in LEG_JOBS {
        let body = job_body(&workflow, job);
        let invocation = format!("scripts/ci-local.sh {leg}");
        assert!(
            run_commands(&body).iter().any(|run| run == &invocation),
            "ci.yml's `{job}` job does not run `{invocation}`. The gate command \
             lives in the script; a job that runs it inline instead is the drift \
             this file exists to stop:\n{body}"
        );
    }
}

#[test]
fn no_leg_job_runs_a_gate_command_of_its_own() {
    // The other half of the check above: a job may CALL the script as often as
    // it likes, but a `cargo` command inline is a second copy of a gate.
    let workflow = workflow();
    let mut inline = Vec::new();
    for job in LEG_JOBS
        .iter()
        .map(|(_, job)| *job)
        .collect::<BTreeSet<_>>()
    {
        for command in run_commands(&job_body(&workflow, job)) {
            if command.starts_with("cargo ") && !command.starts_with("cargo install ") {
                inline.push(format!("  {job}: `{command}`"));
            }
        }
    }
    assert!(
        inline.is_empty(),
        "these ci.yml leg jobs run a cargo gate inline rather than through \
         `scripts/ci-local.sh`, so the local gate no longer covers them (a tool \
         INSTALL is setup, not a gate, and is excused):\n{}",
        inline.join("\n")
    );
}

#[test]
fn the_scripts_leg_list_is_the_workflows_job_list() {
    let legs: BTreeSet<String> = declared_legs().into_iter().collect();
    let mapped: BTreeSet<String> = LEG_JOBS
        .iter()
        .map(|(leg, _)| (*leg).to_string())
        .chain(LOCAL_ONLY.iter().map(|leg| (*leg).to_string()))
        .collect();
    assert_eq!(
        legs, mapped,
        "`scripts/ci-local.sh`'s `LEGS=` and this file's LEG_JOBS/LOCAL_ONLY \
         disagree. A leg added to the script and to no job is a gate CI does not \
         run; a job listed here and absent from the script is a gate nobody can \
         run locally"
    );

    let jobs: BTreeSet<String> = workflow_jobs(&workflow()).into_iter().collect();
    let expected: BTreeSet<String> = LEG_JOBS
        .iter()
        .map(|(_, job)| (*job).to_string())
        .chain(META_JOBS.iter().map(|job| (*job).to_string()))
        .collect();
    assert_eq!(
        jobs, expected,
        "ci.yml's jobs are not the jobs this file describes. A new job is either \
         a leg — give it a row in LEG_JOBS and a function in the script — or \
         meta, like `changes` and `check`, in which case say so in META_JOBS"
    );
}

#[test]
fn the_local_only_legs_are_local_only() {
    let workflow = workflow();
    for leg in LOCAL_ONLY {
        assert!(
            !workflow.contains(&format!("scripts/ci-local.sh {leg}")),
            "`{leg}` is declared local-only — it stands IN FOR a CI leg rather \
             than being one (the windows cross-check compiles for \
             x86_64-pc-windows-msvc and runs nothing). Running it in CI beside \
             the real windows suite is at best redundant; running it INSTEAD is \
             a green tick for a suite that never ran"
        );
    }
    let script = script();
    let declared = script
        .lines()
        .find(|line| line.starts_with("LOCAL_ONLY="))
        .expect("ci-local.sh must declare `LOCAL_ONLY=` — this file reads it");
    for leg in LOCAL_ONLY {
        assert!(
            declared.contains(leg),
            "`{leg}` is local-only here and not in the script: {declared}"
        );
    }
}

#[test]
fn every_declared_leg_has_a_function() {
    // The script refuses to start if this is false, which helps the person who
    // runs it and nobody who does not. Here it is a suite failure.
    let script = script();
    for leg in declared_legs() {
        let function = format!("leg_{}() {{", leg.replace('-', "_"));
        assert!(
            script.contains(&function),
            "`{leg}` is in `LEGS=` but `{function}` is not in the script"
        );
    }
}
