//! The release gate is not weaker than the CI gate (Order 24, N27's lane).
//!
//! `release.yml` runs on a tag push, once per release, and a tag can be pushed
//! from a commit no PR ever saw. So the job graph in that file is the last thing
//! standing between a tree and five publishing channels, several of them
//! one-way — and twice now it has been the WEAKER instrument:
//!
//! - v0.32.0 was tagged, gated green and published while `ci.yml` on the
//!   identical sha was red on both ubuntu and windows, because the release gate
//!   ran plain `cargo test` where CI ran nextest;
//! - v0.37.0 was tagged and published on a tree whose windows CI leg had been
//!   red for days, because the release gate was ubuntu-only where CI's matrix
//!   was not.
//!
//! Both were fixed by making the release side match the CI side, and both fixes
//! were recorded in `release.yml` as a two-sided rule in a comment: *change
//! these two legs and ci.yml's together*. A rule in a comment is what the two
//! releases above had. This file is the rule with a machine behind it.
//!
//! # What this file verifies
//!
//! 1. **Every CI leg has a release-side twin.** `ci.yml`'s five real legs —
//!    `test`, `wasm`, `fmt`, `clippy`, `audit` — each have a job in
//!    `release.yml` running the same command. A leg added to CI and not to the
//!    release reds here.
//! 2. **The commands are identical, not merely similar.** Read out of both
//!    files and compared as strings, which is what "character for character"
//!    has to mean to be checkable.
//! 3. **The publishing jobs wait on all of them.** A leg that runs but that
//!    nothing needs cannot stop a publish, which is the failure mode with no
//!    outward symptom: a green tick beside a job whose verdict was discarded.
//! 4. **`fmt` and `clippy` carry no `RUSTUP_TOOLCHAIN` on either side.** Both
//!    resolve through `rust-toolchain.toml` on purpose (N21) — the lint set and
//!    the formatter's defaults move every six weeks, and an unpinned gate that
//!    denies warnings goes red for a tree nobody edited. A job-level
//!    `RUSTUP_TOOLCHAIN` would outrank the file and reintroduce exactly that,
//!    silently, which is also why these are separate jobs rather than steps
//!    inside `gate` (which declares one).
//!
//! # What this file does NOT verify
//!
//! That either workflow is correct, or that the legs are the right legs. It
//! checks that the two files agree about them — the property whose absence
//! published two releases it should not have.

use std::path::PathBuf;

fn workflow(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.github/workflows")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// The legs both files must run, by the `run:` command that IS the leg.
///
/// The command rather than the job name, because the name is a label and the
/// command is the gate: `release.yml` runs the suite legs inside its `gate` job
/// while `ci.yml` runs them in `test`, and that difference is deliberate and
/// harmless. What must not differ is what gets run.
const LEGS: &[(&str, &str)] = &[
    ("the suite", "cargo nextest run --workspace"),
    ("the doc-tests", "cargo test --workspace --doc"),
    ("formatting", "cargo fmt --all --check"),
    (
        "clippy",
        "cargo clippy --workspace --all-targets -- -D warnings",
    ),
    ("the advisory database", "cargo audit --deny unsound"),
];

/// The lines of `text` that are a step's `run:` command, one-liners only —
/// every leg above is written as one, on both sides.
fn run_commands(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim_start().strip_prefix("run: "))
        .map(|command| command.trim().to_string())
        .collect()
}

/// One job's body, by its key. A job is a line at exactly two spaces of indent;
/// everything under it is indented further, so the body runs to the next line
/// that starts at that depth (the next job, or the comment introducing it).
fn job_body(text: &str, job: &str) -> String {
    let at_job_depth = |line: &str| {
        line.starts_with("  ") && !line.chars().nth(2).is_some_and(char::is_whitespace)
    };
    let mut lines = text.lines().skip_while(|line| *line != format!("  {job}:"));
    let head = lines
        .next()
        .unwrap_or_else(|| panic!("no `{job}:` job at job depth"));
    std::iter::once(head)
        .chain(lines.take_while(|line| !at_job_depth(line)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_ci_leg_runs_on_the_release_side_too() {
    let ci = run_commands(&workflow("ci.yml"));
    let release = run_commands(&workflow("release.yml"));
    let mut missing = Vec::new();
    for (leg, command) in LEGS {
        assert!(
            ci.iter().any(|run| run == command),
            "`{leg}` is listed here as a CI leg but ci.yml does not run `{command}` — \
             the leg was reworded or dropped, and this file's roster is now describing \
             a workflow that no longer exists"
        );
        if !release.iter().any(|run| run == command) {
            missing.push(format!("  {leg}: `{command}`"));
        }
    }
    assert!(
        missing.is_empty(),
        "release.yml does not run {} of ci.yml's legs. A tag can be pushed from a \
         commit no PR ever saw, so a leg that decides a COMMIT is green has to decide \
         a RELEASE is publishable too — v0.32.0 and v0.37.0 both published on trees CI \
         was red on, each time because this side was the weaker instrument:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn the_release_legs_are_reachable_from_every_publishing_job() {
    // A leg nothing waits on is the failure with no symptom: it runs, it goes
    // red, and the publish proceeds beside it.
    let release = workflow("release.yml");
    let gated: Vec<&str> = release
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("needs: ["))
        .collect();
    assert!(
        gated.len() >= 3,
        "release.yml declares only {} multi-job `needs:` — the artifact jobs are \
         supposed to wait on the whole gate: {gated:?}",
        gated.len()
    );
    for job in ["gate", "fmt", "clippy", "audit"] {
        let waiting = gated
            .iter()
            .filter(|needs| {
                needs.contains(&format!("[{job},"))
                    || needs.contains(&format!(" {job},"))
                    || needs.contains(&format!(" {job}]"))
            })
            .count();
        assert_eq!(
            waiting, 3,
            "the `{job}` leg is waited on by {waiting} of the three artifact jobs \
             (`build`, `vsix`, `wasm`). A gate job nothing needs cannot stop a \
             publish:\n{gated:#?}"
        );
    }
}

#[test]
fn the_formatter_and_clippy_legs_pin_through_the_toolchain_file_on_both_sides() {
    // N21's argument, and the reason `fmt`/`clippy` are separate jobs on the
    // release side rather than steps inside `gate`: `gate` declares
    // `RUSTUP_TOOLCHAIN: stable`, which outranks `rust-toolchain.toml`, and a
    // formatter resolved to a moving channel makes a byte-stable gate a
    // seasonal one.
    for name in ["ci.yml", "release.yml"] {
        let text = workflow(name);
        for (job, command) in [
            ("fmt", "cargo fmt --all --check"),
            (
                "clippy",
                "cargo clippy --workspace --all-targets -- -D warnings",
            ),
        ] {
            let body = job_body(&text, job);
            assert!(
                body.contains(command),
                "{name}'s `{job}` job does not run `{command}`:\n{body}"
            );
            assert!(
                !body.contains("RUSTUP_TOOLCHAIN"),
                "{name}'s `{job}` job declares RUSTUP_TOOLCHAIN, which outranks \
                 rust-toolchain.toml — the pin is what makes this leg's answer stable \
                 across a toolchain release (N21):\n{body}"
            );
        }
    }
}
