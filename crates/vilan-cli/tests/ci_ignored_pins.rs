//! The ignored set, gated from two sides.
//!
//! **The weekly leg** (backlog N27), held to the decisions that make it worth
//! having. Nothing else in the tree runs `.github/workflows/ignored-pins.yml` —
//! it fires on a cron, once a week, and a leg that has quietly stopped
//! excluding the 247-second pin, stopped being non-blocking, or started using
//! nextest's deprecated flag spelling would first be noticed by whoever waits
//! on it. So these pins read the workflow and hold the four decisions in it,
//! plus the one fact outside it the leg depends on: that the test it filters by
//! name is still a test.
//!
//! **The reasons themselves** (backlog N31). Two house rules meet at an
//! `#[ignore]`: a known-but-unfixed bug is pinned `#[ignore]`d, and an open
//! defect is not tracked unless it has an item. Nothing enforced the join, and
//! audit runs 2, 3 and 4 each found an ignored pin whose defect lived nowhere
//! but its reason string — a bug the tracker had never heard of, discoverable
//! only by reading test attributes. So every reason must name a tracker item,
//! or be one of the few ignores that are deliberately not bugs.

use std::path::PathBuf;
use std::process::Command;

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

// ── N31: every `#[ignore]` reason names a tracker item ────────────────────────

/// The ignores that are deliberately NOT bugs, and so have no item to name.
/// Listed by their exact reason string rather than matched by a pattern: a
/// pattern is how "run deliberately" quietly becomes an escape hatch anyone can
/// spell their way into, and the whole point of this gate is that adding a
/// member is a decision somebody makes on purpose, in a diff.
///
/// The first three are cost and tooling, not defects — nothing is broken, and
/// running them in the ordinary suite would buy a wrong answer (a timing
/// measurement under suite load) or none at all (a build tool that is not
/// installed).
const DELIBERATE_NON_BUG_IGNORES: &[&str] = &[
    // The perf baseline, in both places it is measured (the CLI's corpus timing
    // and the LSP's edit latency): minutes of measurement whose number means
    // nothing on a machine running forty other test binaries.
    "the performance baseline: minutes of measurement, run deliberately \
     (proposal/perf-baseline.md §3)",
    // The leak soak: thousands of analyses per corpus, same reason.
    "the leak soak: thousands of analyses per corpus, run deliberately \
     (proposal/leak-soak.md §5)",
    // A tool gate: the book-heading differential needs a pinned `mdbook` on
    // PATH, which CI has and a contributor's machine may not.
    "needs the pinned `mdbook` v0.5.4 on PATH: builds the book and compares \
     every heading id to mdbook_heading_ids",
];

/// Whether `reason` carries something shaped like a tracker item id — one or
/// more capitals followed immediately by digits (`B154`, `E102`, `N31`). The
/// shape, not a roster: this gate lives in the compiler repo and the tracker
/// lives in another, so it can insist that a reason POINT somewhere without
/// pretending to know what exists there.
fn names_a_tracker_item(reason: &str) -> bool {
    let characters: Vec<char> = reason.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        if !characters[index].is_ascii_uppercase() {
            index += 1;
            continue;
        }
        let mut after_capitals = index;
        while after_capitals < characters.len() && characters[after_capitals].is_ascii_uppercase() {
            after_capitals += 1;
        }
        if characters
            .get(after_capitals)
            .is_some_and(char::is_ascii_digit)
        {
            return true;
        }
        index = after_capitals;
    }
    false
}

/// Every tracked `.rs` file, as `(repo-relative name, contents)`. `git
/// ls-files` is the enumerator on purpose: it is exactly the committed tree, so
/// the sweep can never wander into `target/` or into a sibling worktree under
/// `.claude/` (both ignored) and mistake somebody else's branch for this one.
fn tracked_rust_sources() -> Vec<(String, String)> {
    let repository_root = repository_root();
    let listing = Command::new("git")
        .args(["ls-files", "-z", "*.rs"])
        .current_dir(&repository_root)
        .output()
        .expect("git ls-files");
    assert!(listing.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&listing.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .filter_map(|name| {
            let text = std::fs::read_to_string(repository_root.join(name)).ok()?;
            Some((name.to_string(), text))
        })
        .collect()
}

/// Every `#[ignore]` attribute in `text`, as `(1-based line, reason)` — `None`
/// when the attribute carries no reason at all.
///
/// An attribute is recognized only where one is written: at the start of a
/// line, whitespace aside. That is what keeps the sweep off the many `#[ignore]`
/// mentions in this tree's prose — doc comments explaining the house rule,
/// including the ones in this very file — without needing to parse Rust.
fn ignore_attributes(text: &str) -> Vec<(usize, Option<String>)> {
    const ATTRIBUTE: &str = "#[ignore";
    let bytes = text.as_bytes();
    let mut attributes = Vec::new();
    let mut line = 1;
    let mut still_leading_whitespace = true;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            line += 1;
            still_leading_whitespace = true;
            index += 1;
            continue;
        }
        if !bytes[index].is_ascii_whitespace() {
            if still_leading_whitespace && text[index..].starts_with(ATTRIBUTE) {
                let (reason, after) = read_attribute(text, index + ATTRIBUTE.len());
                attributes.push((line, reason));
                line += text[index..after].matches('\n').count();
                index = after;
                // Resume AT the character after the bracket, never past it: a
                // newline swallowed here loses a line for everything below.
                still_leading_whitespace = false;
                continue;
            }
            still_leading_whitespace = false;
        }
        index += 1;
    }
    attributes
}

/// Reads one attribute from just past `#[ignore` to its closing `]`, returning
/// the string literal inside it (decoded) and the offset after the bracket. The
/// scan understands string literals because a reason may span lines with `\`
/// continuations and may contain a bracket.
fn read_attribute(text: &str, from: usize) -> (Option<String>, usize) {
    let bytes = text.as_bytes();
    let mut reason = None;
    let mut index = from;
    while index < bytes.len() {
        match bytes[index] {
            b']' => return (reason, index + 1),
            b'"' => {
                let (literal, after) = read_string_literal(text, index);
                reason = Some(literal);
                index = after;
            }
            _ => index += 1,
        }
    }
    (reason, bytes.len())
}

/// The contents of the Rust string literal starting at the quote at `from`,
/// with its escapes resolved, and the offset just past its closing quote. A
/// backslash before a newline swallows the break and the next line's
/// indentation, exactly as rustc does — which is what lets a long reason be
/// written across lines and still compare equal to a one-line allowlist entry.
fn read_string_literal(text: &str, from: usize) -> (String, usize) {
    let bytes = text.as_bytes();
    let mut literal = String::new();
    let mut index = from + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return (literal, index + 1),
            b'\\' => {
                index += 1;
                match bytes.get(index) {
                    Some(b'n') => literal.push('\n'),
                    Some(b't') => literal.push('\t'),
                    Some(b'\n') => {
                        index += 1;
                        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                            index += 1;
                        }
                        continue;
                    }
                    Some(_) => {
                        let rest = &text[index..];
                        let character = rest.chars().next().expect("a character follows `\\`");
                        literal.push(character);
                        index += character.len_utf8();
                        continue;
                    }
                    None => return (literal, bytes.len()),
                }
                index += 1;
            }
            _ => {
                let rest = &text[index..];
                let character = rest.chars().next().expect("a character at a boundary");
                literal.push(character);
                index += character.len_utf8();
            }
        }
    }
    (literal, bytes.len())
}

// The gate itself. A pin that says only what is wrong, and not WHERE that is
// written down, is how a defect ends up living in a test attribute — which is
// the shape three consecutive audit runs found and the reason this exists.
#[test]
fn every_ignored_pin_names_a_tracker_item_or_is_a_declared_non_bug() {
    let mut offenders = Vec::new();
    for (name, text) in tracked_rust_sources() {
        for (line, reason) in ignore_attributes(&text) {
            let Some(reason) = reason else {
                offenders.push(format!("{name}:{line}: `#[ignore]` with no reason at all"));
                continue;
            };
            if names_a_tracker_item(&reason) {
                continue;
            }
            if DELIBERATE_NON_BUG_IGNORES.contains(&reason.as_str()) {
                continue;
            }
            offenders.push(format!("{name}:{line}: {reason:?}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "an `#[ignore]` reason must name its tracker item (`B154`, `E102`, …) so the \
         defect lives somewhere other than this attribute — or, for an ignore that is \
         deliberately not a bug, be added to `DELIBERATE_NON_BUG_IGNORES` here, on \
         purpose. Offending:\n  {}",
        offenders.join("\n  ")
    );
}

// The sweep is only worth what its predicate is worth, and a predicate that
// answered `true` to everything would make the gate above green forever.
#[test]
fn the_item_id_shape_accepts_an_id_and_refuses_prose() {
    for reason in [
        "B154 — the internal `NativeMap::insert` frees the caller's value",
        "E102 residue",
        "waiting on N31",
        "C11's predicate, narrowed",
    ] {
        assert!(
            names_a_tracker_item(reason),
            "should name an item: {reason}"
        );
    }
    for reason in [
        "needs the pinned mdbook on PATH",
        "run deliberately: minutes of measurement",
        "OPEN, not yet filed",
        "b154 in lower case is not an id",
        "",
    ] {
        assert!(
            !names_a_tracker_item(reason),
            "should NOT count as naming an item: {reason}"
        );
    }
}

// And the scanner: it must find the attribute where one is written, across the
// line break a long reason takes, and must NOT find the ones this tree's prose
// talks about — a false positive there would redden the gate over a comment.
#[test]
fn the_sweep_reads_attributes_and_not_prose_about_them() {
    let source = "\
/// A known-but-unfixed bug is pinned `#[ignore]`d and un-ignored when fixed.
#[test]
#[ignore = \"B1: a reason \\
            written across lines\"]
fn pinned() {}

#[test]
#[ignore]
fn bare() {}
";
    let found = ignore_attributes(source);
    assert_eq!(
        found,
        vec![
            (3, Some("B1: a reason written across lines".to_string())),
            (8, None),
        ],
        "the doc comment's `#[ignore]` is prose, not an attribute"
    );
}
