//! The train, scripted (proposal/releases.md §7.2, backlog §L item 2):
//! `scripts/cut-release.sh` and `scripts/fold-release.sh`.
//!
//! releases.md §7.2 is the authority for what a cut is; these two scripts are
//! its executor, and nothing else in the tree exercises them — a cut happens
//! once a week, on one machine, and a script that has quietly stopped ordering
//! entries would first be noticed by a reader of a published release. So the
//! pins here run both scripts, in their read-only modes, against fixture
//! repositories this file builds: a tiny git repo with a fixture `CHANGELOG.md`
//! for the cut, and a second one missing every fold precondition in turn.
//!
//! Both scripts locate their repository as `dirname $0/..`, which is what makes
//! this testable at all: a copy of the script placed in a fixture repo's
//! `scripts/` operates on that repo and nothing else. No test reaches the
//! network: a fixture's `origin` is a name pointing at a path that never
//! exists (nothing fetches or pushes), and the `gh` a script finds on PATH is
//! the fixture's own shim, answering the CI verdict the test chose. The fold
//! fixture removes that origin again — its premise is a repository missing
//! every precondition, and the checks that need one report themselves
//! skipped, which is itself pinned.
//!
//! unix-only: both are POSIX shell scripts and the Windows leg of CI has no
//! shell to run them with (`tests/brew_formula.rs` is gated the same way).
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The fixture's `gh` (backlog L17): no network. The cut consumes `gh run
/// list --jq …` as one cooked verdict string, so the shim prints exactly
/// that — whatever `$VILAN_FIXTURE_CI` holds, "completed success" when the
/// test says nothing (every pre-L17 pin then doubles as a green-CI pin) —
/// and "unreachable" makes it fail the way an offline or unauthenticated
/// gh does.
const GH_SHIM: &str = r#"#!/bin/sh
set -eu
verdict="${VILAN_FIXTURE_CI:-completed success}"
if [ "$verdict" = unreachable ]; then
    echo "fixture gh: unreachable" >&2
    exit 1
fi
printf '%s\n' "$verdict"
"#;

/// A throwaway git repository holding a copy of both scripts, an initial
/// commit, and whatever changelog the test wants. Its `HOME` and git config are
/// redirected at the scratch directory so nothing here can read — or be
/// perturbed by — the machine's own git identity, signing setup, or installed
/// toolchain. Its `origin` is a name only (the path never exists; nothing
/// here fetches or pushes), and the `gh` answering for that origin is the
/// shim in the fixture's own `bin/`, first on `PATH`.
struct Fixture {
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new(name: &str, changelog: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "vilan-release-scripts-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        fs::create_dir_all(&home).expect("create the scratch home");
        fs::create_dir_all(root.join("scripts")).expect("create scripts/");
        for script in ["cut-release.sh", "fold-release.sh"] {
            fs::copy(
                repository_root().join("scripts").join(script),
                root.join("scripts").join(script),
            )
            .expect("copy the script into the fixture");
        }
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create the fixture bin/");
        write_shim(&bin.join("gh"), GH_SHIM);
        let fixture = Fixture { root, home, bin };
        fixture.git(&["init", "--initial-branch=next"]);
        fixture.git(&["remote", "add", "origin", "/var/empty/fixture-origin.git"]);
        fs::write(fixture.root.join("CHANGELOG.md"), changelog).expect("write the changelog");
        // A second tracked file so the initial commit is not records-only —
        // the sweep notes a changelog entry that arrived without code, and a
        // fixture should not trip a note it is not testing.
        fs::write(
            fixture.root.join("source.txt"),
            "the code the entries describe\n",
        )
        .expect("write the source stand-in");
        fixture.git(&["add", "CHANGELOG.md", "source.txt"]);
        fixture.git(&["commit", "-m", "the lane's own commit"]);
        fixture
    }

    fn git(&self, arguments: &[&str]) -> Output {
        let output = self
            .command("git")
            .args(arguments)
            .output()
            .expect("run git in the fixture");
        assert!(
            output.status.success(),
            "git {arguments:?} failed in the fixture:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        // The fixture's bin/ (the `gh` shim) shadows the machine's own gh —
        // nothing a script resolves through PATH may reach the network.
        let path = std::env::join_paths(std::iter::once(self.bin.clone()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .expect("assemble the fixture PATH");
        command
            .current_dir(&self.root)
            .env("PATH", path)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
        command
    }

    /// Runs one of the copied scripts and returns `(exit ok, stdout+stderr)`.
    /// The fixture's CI answers green — the cut's default world.
    fn script(&self, name: &str, arguments: &[&str]) -> (bool, String) {
        self.script_with_ci(name, arguments, "completed success")
    }

    /// `script`, with the fixture's CI answering `verdict` — the cooked
    /// string the real `gh run list --jq` pipeline yields ("completed
    /// failure", "in_progress ", "none", …; "unreachable" makes the shim
    /// fail the way an offline gh does).
    fn script_with_ci(&self, name: &str, arguments: &[&str], verdict: &str) -> (bool, String) {
        let output = self
            .command("sh")
            .arg(format!("scripts/{name}"))
            .args(arguments)
            .env("VILAN_FIXTURE_CI", verdict)
            .output()
            .unwrap_or_else(|error| panic!("run scripts/{name}: {error}"));
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        (output.status.success(), text)
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.join(relative)).expect("read a fixture file")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// One entry of each family, deliberately out of §7.2's order, and every
/// separator irregularity a week of lane merges actually produces: a `---`
/// with no blank line after it, a doubled rule where two lanes each brought
/// their own, and a trailing rule before the next section's heading.
const SCRAMBLED: &str = "\
# Changelog

Preamble that must not move.

## Unreleased

<!-- family: tooling -->
**A tooling entry.** What the toolchain does better.

---

<!-- family: breaking -->
**A breaking entry.** What stopped compiling.

A second paragraph, which belongs to the breaking entry.

---
<!-- family: feature -->
**A feature entry.** What is newly possible.

---

---

<!-- family: miscompile -->
**A miscompile entry.** What the compiler was wrong about.

---

## v0.1.0 — 2026-01-01

**An older entry.** It is released and must not move.
";

#[test]
fn the_cut_retitles_the_section_and_orders_it_by_family() {
    let fixture = Fixture::new("order", SCRAMBLED);
    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.9",
        ],
    );
    assert!(ok, "the cut refused a fully classified section:\n{report}");

    // Sweep (a): every entry traces to the commit that introduced it.
    assert_eq!(
        traced_entries(&report),
        4,
        "expected four traced entries:\n{report}"
    );

    // The whole file, byte for byte. The retitle, §7.2's family order, the
    // authored text untouched, exactly one rule between neighbours, and the
    // released section below it left alone.
    let expected = "\
# Changelog

Preamble that must not move.

## v9.9.9 — 2026-01-02

<!-- family: breaking -->
**A breaking entry.** What stopped compiling.

A second paragraph, which belongs to the breaking entry.

---

<!-- family: miscompile -->
**A miscompile entry.** What the compiler was wrong about.

---

<!-- family: feature -->
**A feature entry.** What is newly possible.

---

<!-- family: tooling -->
**A tooling entry.** What the toolchain does better.

## v0.1.0 — 2026-01-01

**An older entry.** It is released and must not move.
";
    assert_eq!(
        fs::read_to_string(&out).expect("read the proposed changelog"),
        expected
    );
    // `--out` writes elsewhere and nothing else moves.
    assert!(fixture.read("CHANGELOG.md").contains("## Unreleased"));
}

#[test]
fn the_cut_refuses_an_entry_it_cannot_classify_instead_of_guessing() {
    let unclassifiable = SCRAMBLED.replace(
        "<!-- family: feature -->\n**A feature entry.**",
        "**A feature entry.**",
    );
    let fixture = Fixture::new("unclassified", &unclassifiable);
    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.9",
        ],
    );
    assert!(
        !ok,
        "the cut ordered a section it cannot classify:\n{report}"
    );
    assert!(
        report.contains("carries no `<!-- family: ... -->` marker: A feature entry."),
        "the refusal must name the entry it cannot place:\n{report}"
    );
    assert!(
        report.contains("refusing to cut"),
        "the refusal must say nothing was changed:\n{report}"
    );
    // Refusing is not a reason to stop reporting: the sweep still runs, so one
    // run tells the operator everything that is wrong.
    assert_eq!(traced_entries(&report), 4, "{report}");
    assert!(!out.exists(), "a refused cut must write nothing");

    // An unknown family is refused the same way, naming what it does not know.
    let mistyped = SCRAMBLED.replace("family: feature", "family: refactor");
    let fixture = Fixture::new("unknown-family", &mistyped);
    let (ok, report) = fixture.script("cut-release.sh", &["--date", "2026-01-02", "9.9.9"]);
    assert!(!ok, "an unknown family was accepted:\n{report}");
    assert!(report.contains("the unknown family `refactor`"), "{report}");
}

/// The sweep's own `ok` lines — the CI verdict prints an `ok` too, and the
/// assertions using this count traced ENTRIES, not everything green.
fn traced_entries(report: &str) -> usize {
    report
        .lines()
        .filter(|line| line.starts_with("  ok    ") && !line.contains("ci.yml"))
        .count()
}

/// The 1-based line `needle` sits on, for a refusal that must name it.
fn line_of(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|line| line == needle)
        .map(|index| index + 1)
        .unwrap_or_else(|| panic!("no line of the fixture reads `{needle}`"))
}

fn refuse(name: &str, changelog: &str) -> String {
    let fixture = Fixture::new(name, changelog);
    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.9",
        ],
    );
    assert!(!ok, "the cut accepted a section it must refuse:\n{report}");
    assert!(
        report.contains("refusing to cut"),
        "the refusal must say nothing was changed:\n{report}"
    );
    assert!(!out.exists(), "a refused cut must write nothing");
    assert!(fixture.read("CHANGELOG.md").contains("## Unreleased"));
    report
}

/// The 2026-08-20 shape (backlog L11): a `<!-- family: ... -->` line that a
/// CHANGELOG merge-union left with nothing under it — blank lines, then the
/// next rule. The parser used to let the rule clear the pending family and
/// the dry-run stayed green, so the dangling comment would have ridden into
/// the release section. A marker that reaches a rule, another marker of its
/// kind, or the section's end without a bold head is refused, by line.
#[test]
fn the_cut_refuses_a_marker_that_opens_no_entry() {
    let stranded = SCRAMBLED.replace(
        "---\n\n---\n\n<!-- family: miscompile -->",
        "---\n\n<!-- family: diagnostics -->\n\n\n---\n\n<!-- family: miscompile -->",
    );
    let line = line_of(&stranded, "<!-- family: diagnostics -->");
    let report = refuse("orphan-family", &stranded);
    assert!(
        report.contains(&format!(
            "marker `<!-- family: diagnostics -->` at line {line} opens no entry"
        )),
        "the refusal must name the stranded marker and its line:\n{report}"
    );
    // Refusing is not a reason to stop reporting: the sweep still traces the
    // four entries the section does hold.
    assert_eq!(traced_entries(&report), 4, "{report}");

    // A `commit:` marker is a marker too.
    let stranded = SCRAMBLED.replace(
        "---\n\n---\n\n<!-- family: miscompile -->",
        "---\n\n<!-- commit: 0123abcd -->\n---\n\n<!-- family: miscompile -->",
    );
    let line = line_of(&stranded, "<!-- commit: 0123abcd -->");
    let report = refuse("orphan-commit", &stranded);
    assert!(
        report.contains(&format!(
            "marker `<!-- commit: 0123abcd -->` at line {line} opens no entry"
        )),
        "{report}"
    );

    // Two family markers in a row: the first opens nothing.
    let doubled = SCRAMBLED.replace(
        "<!-- family: feature -->\n**A feature entry.**",
        "<!-- family: diagnostics -->\n<!-- family: feature -->\n**A feature entry.**",
    );
    let line = line_of(&doubled, "<!-- family: diagnostics -->");
    let report = refuse("doubled-family", &doubled);
    assert!(
        report.contains(&format!(
            "marker `<!-- family: diagnostics -->` at line {line} opens no entry"
        )),
        "{report}"
    );
    // The second marker is the one the entry carries: it is placed, and the
    // stranded first marker is the only red.
    assert!(
        !report.contains("carries no `<!-- family: ... -->` marker"),
        "{report}"
    );
    assert_eq!(report.matches("  RED   ").count(), 1, "{report}");

    // A marker the next section's heading cuts off, and one the file ends on.
    let at_heading = SCRAMBLED.replace(
        "---\n\n## v0.1.0",
        "---\n\n<!-- family: diagnostics -->\n## v0.1.0",
    );
    let line = line_of(&at_heading, "<!-- family: diagnostics -->");
    let report = refuse("orphan-at-heading", &at_heading);
    assert!(
        report.contains(&format!(
            "marker `<!-- family: diagnostics -->` at line {line} opens no entry"
        )),
        "{report}"
    );
    let at_end = format!(
        "{}<!-- family: diagnostics -->\n",
        &SCRAMBLED[..SCRAMBLED.find("## v0.1.0").expect("the released section")]
    );
    let line = line_of(&at_end, "<!-- family: diagnostics -->");
    let report = refuse("orphan-at-end", &at_end);
    assert!(
        report.contains(&format!(
            "marker `<!-- family: diagnostics -->` at line {line} opens no entry"
        )),
        "{report}"
    );
}

/// A marker sits directly above its head — the changelog's own writing note
/// says so, every marker in the tree does so, and the only marker a blank
/// line ever followed was the 2026-08-20 orphan. So a blank between the two
/// is refused rather than tolerated, and the head below it is then an entry
/// with no family: both reds name the one fault, from each side.
#[test]
fn the_cut_refuses_a_marker_parted_from_its_head_by_a_blank_line() {
    let parted = SCRAMBLED.replace(
        "<!-- family: feature -->\n**A feature entry.**",
        "<!-- family: feature -->\n\n**A feature entry.**",
    );
    let line = line_of(&parted, "<!-- family: feature -->");
    let report = refuse("parted-marker", &parted);
    assert!(
        report.contains(&format!(
            "marker `<!-- family: feature -->` at line {line} opens no entry"
        )),
        "{report}"
    );
    assert!(
        report.contains("carries no `<!-- family: ... -->` marker: A feature entry."),
        "{report}"
    );
}

/// A marker followed by prose instead of a bold head: the prose was already
/// refused as text that begins no entry, and the marker above it is now
/// refused as well.
#[test]
fn the_cut_refuses_a_marker_followed_by_prose() {
    let prose = SCRAMBLED.replace(
        "<!-- family: feature -->\n**A feature entry.** What is newly possible.",
        "<!-- family: feature -->\nA feature entry, with no bold head to open it.",
    );
    let line = line_of(&prose, "<!-- family: feature -->");
    let report = refuse("marker-then-prose", &prose);
    assert!(
        report.contains(
            "text under `## Unreleased` that begins no entry: A feature entry, with no bold head to open it."
        ),
        "{report}"
    );
    assert!(
        report.contains(&format!(
            "marker `<!-- family: feature -->` at line {line} opens no entry"
        )),
        "{report}"
    );
}

/// The legitimate shapes: a `commit:` marker above or below the `family:`
/// marker, each directly above the head. Both cut, and the rewrite puts the
/// two lines in one order.
#[test]
fn the_cut_accepts_a_commit_marker_on_either_side_of_the_family_marker() {
    let fixture = Fixture::new("commit-marker-order", SCRAMBLED);
    let sha = String::from_utf8_lossy(&fixture.git(&["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();
    let claimed = SCRAMBLED
        .replace(
            "<!-- family: breaking -->",
            &format!("<!-- commit: {sha} -->\n<!-- family: breaking -->"),
        )
        .replace(
            "<!-- family: feature -->",
            &format!("<!-- family: feature -->\n<!-- commit: {sha} -->"),
        );
    fs::write(fixture.root.join("CHANGELOG.md"), claimed).expect("write the changelog");
    fixture.git(&["commit", "-am", "records: name the commits"]);

    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.9",
        ],
    );
    assert!(
        ok,
        "the cut refused a commit marker beside a family marker:\n{report}"
    );
    assert_eq!(traced_entries(&report), 4, "{report}");
    let proposed = fs::read_to_string(&out).expect("read the proposed changelog");
    for family in ["breaking", "feature"] {
        let expected =
            format!("<!-- commit: {sha} -->\n<!-- family: {family} -->\n**A {family} entry.**");
        assert!(
            proposed.contains(&expected),
            "expected the {family} entry to carry both markers, commit first:\n{proposed}"
        );
    }
}

// --- The deprecation lifetime sweep (proposal/deprecation.md §3) ------------
//
// `<!-- deprecates: KEY -->` / `<!-- removes: KEY -->` above an entry's head:
// a `removes:` under Unreleased cuts only when a RELEASED section carries the
// matching `deprecates:` — one minor of warning, with no version arithmetic,
// because every train is a minor. These pins ride the same fixture repos as
// the family sweep's; note the cut VERSION matters here (9.9.0, not 9.9.9 —
// a patch cut refuses the markers outright, which is itself pinned below).

/// `refuse`, but cutting the MINOR 9.9.0 (the lifetime sweep's ordinary
/// train; `refuse`'s 9.9.9 would trip the patch rule first).
fn refuse_minor(name: &str, changelog: &str) -> String {
    let fixture = Fixture::new(name, changelog);
    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.0",
        ],
    );
    assert!(!ok, "the cut accepted a section it must refuse:\n{report}");
    assert!(
        report.contains("refusing to cut"),
        "the refusal must say nothing was changed:\n{report}"
    );
    assert!(!out.exists(), "a refused cut must write nothing");
    assert!(fixture.read("CHANGELOG.md").contains("## Unreleased"));
    report
}

#[test]
fn the_cut_refuses_a_removal_whose_deprecation_never_shipped() {
    // The plant-proven case §3 exists for: a removal jumping the window. No
    // released section carries `deprecates: std::old::thing`, so the cut is
    // REFUSED and the key printed — never guessed.
    let jumped = SCRAMBLED.replace(
        "<!-- family: breaking -->",
        "<!-- family: breaking -->\n<!-- removes: std::old::thing -->",
    );
    let report = refuse_minor("removal-unshipped", &jumped);
    assert!(
        report.contains(
            "RED   removes: std::old::thing - no RELEASED section carries \
             `deprecates: std::old::thing`, so its warning never shipped"
        ),
        "the refusal must name the key and the missing warning:\n{report}"
    );
    assert!(
        report.contains("A breaking entry."),
        "the refusal must name the entry carrying the marker:\n{report}"
    );
}

#[test]
fn a_deprecation_in_the_same_unreleased_section_does_not_license_its_removal() {
    // Warning and removal riding ONE train is exactly the no-window shape the
    // check refuses: the match must sit in a released section.
    let same_train = SCRAMBLED
        .replace(
            "<!-- family: breaking -->",
            "<!-- family: breaking -->\n<!-- removes: std::old::thing -->",
        )
        .replace(
            "<!-- family: tooling -->",
            "<!-- family: tooling -->\n<!-- deprecates: std::old::thing -->",
        );
    let report = refuse_minor("same-train", &same_train);
    assert!(
        report.contains("no RELEASED section carries `deprecates: std::old::thing`"),
        "{report}"
    );
    assert!(
        report.contains("a deprecation in this same Unreleased section does not count"),
        "{report}"
    );
}

#[test]
fn the_cut_accepts_a_removal_whose_deprecation_shipped_and_keeps_the_markers() {
    // The released `deprecates:` licenses the removal; the cut orders,
    // names the shipping train, and the rewrite carries BOTH marker lines
    // into the release section (the CHANGELOG stays the ledger).
    let windowed = SCRAMBLED
        .replace(
            "<!-- family: breaking -->",
            "<!-- family: breaking -->\n<!-- removes: std::old::thing -->",
        )
        .replace(
            "<!-- family: feature -->",
            "<!-- family: feature -->\n<!-- deprecates: std::next::form -->",
        )
        .replace(
            "## v0.1.0 — 2026-01-01\n",
            "## v0.1.0 — 2026-01-01\n\n<!-- deprecates: std::old::thing -->",
        );
    let fixture = Fixture::new("removal-windowed", &windowed);
    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.0",
        ],
    );
    assert!(ok, "the cut refused a windowed removal:\n{report}");
    assert!(
        report.contains("ok    removes: std::old::thing  (deprecated in v0.1.0)"),
        "the sweep must name the train that shipped the warning:\n{report}"
    );
    assert!(
        report.contains("ok    deprecates: std::next::form  (the window opens with this cut)"),
        "{report}"
    );
    let proposed = fs::read_to_string(&out).expect("read the proposed changelog");
    assert!(
        proposed.contains(
            "<!-- family: breaking -->\n<!-- removes: std::old::thing -->\n**A breaking entry.**"
        ),
        "the rewrite must keep the removal marker above its head:\n{proposed}"
    );
    assert!(
        proposed.contains(
            "<!-- family: feature -->\n<!-- deprecates: std::next::form -->\n**A feature entry.**"
        ),
        "the rewrite must keep the deprecation marker above its head:\n{proposed}"
    );
}

#[test]
fn the_cut_reports_shipped_deprecations_still_pending_removal() {
    // A `deprecates:` with no `removes:` yet is NOT an error (§5.2(1) is a
    // floor) — the sweep reports it, key and shipping train, at every cut.
    let pending = SCRAMBLED.replace(
        "## v0.1.0 — 2026-01-01\n",
        "## v0.1.0 — 2026-01-01\n\n<!-- deprecates: std::old::thing -->",
    );
    let fixture = Fixture::new("pending-report", &pending);
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &["--date", "2026-01-02", "--dry-run", "9.9.0"],
    );
    assert!(ok, "a pending deprecation must not red the cut:\n{report}");
    assert!(
        report.contains("deprecations still in their window (report only)"),
        "{report}"
    );
    assert!(
        report.contains("pending  std::old::thing  (deprecated in v0.1.0, not yet removed"),
        "the report names the key and the train that shipped it:\n{report}"
    );
}

#[test]
fn a_patch_cut_refuses_lifetime_markers_outright() {
    // Deprecations and removals ride minors only (releases.md §4: patches are
    // fixes) — `refuse`'s 9.9.9 cut is the patch here.
    let on_a_patch = SCRAMBLED.replace(
        "<!-- family: breaking -->",
        "<!-- family: breaking -->\n<!-- deprecates: std::old::thing -->",
    );
    let report = refuse("patch-lifetime", &on_a_patch);
    assert!(
        report.contains(
            "RED   `deprecates: std::old::thing` on a PATCH cut - deprecations and removals \
             ride minors only (releases.md §4)"
        ),
        "{report}"
    );
}

#[test]
fn a_stranded_lifetime_marker_is_refused_like_any_other() {
    // The L11 discipline extends to the new markers: one parted from its head
    // by a blank line opens no entry, and an empty KEY names nothing to track.
    let stranded = SCRAMBLED.replace(
        "<!-- family: feature -->\n**A feature entry.**",
        "<!-- deprecates: std::old::thing -->\n\n<!-- family: feature -->\n**A feature entry.**",
    );
    let line = line_of(&stranded, "<!-- deprecates: std::old::thing -->");
    let report = refuse_minor("orphan-lifetime", &stranded);
    assert!(
        report.contains(&format!(
            "marker `<!-- deprecates: std::old::thing -->` at line {line} opens no entry"
        )),
        "{report}"
    );

    let empty = SCRAMBLED.replace(
        "<!-- family: feature -->",
        "<!-- family: feature -->\n<!-- removes: -->",
    );
    let line = line_of(&empty, "<!-- removes: -->");
    let report = refuse_minor("empty-key", &empty);
    assert!(
        report.contains(&format!(
            "marker `<!-- removes: -->` at line {line} names no key"
        )),
        "{report}"
    );
}

#[test]
fn the_sweep_reds_an_entry_whose_commit_is_not_an_ancestor_of_the_tag() {
    // The drift §7.1 was written about: an entry filed under Unreleased whose
    // code sits on a branch that never merged. The entry names its commit, and
    // that commit is not on the line being tagged.
    let fixture = Fixture::new("ancestry", SCRAMBLED);
    fixture.git(&["checkout", "-b", "unmerged-lane"]);
    fs::write(fixture.root.join("source.txt"), "work that never merged\n").expect("write");
    fixture.git(&["commit", "-am", "a lane that never merged"]);
    let stranded = String::from_utf8_lossy(&fixture.git(&["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();
    fixture.git(&["checkout", "next"]);

    let claimed = SCRAMBLED.replace(
        "<!-- family: feature -->",
        &format!("<!-- commit: {stranded} -->\n<!-- family: feature -->"),
    );
    fs::write(fixture.root.join("CHANGELOG.md"), claimed).expect("write the changelog");
    fixture.git(&["commit", "-am", "records: file the entry"]);

    let (ok, report) = fixture.script("cut-release.sh", &["--date", "2026-01-02", "9.9.9"]);
    assert!(
        !ok,
        "the sweep passed an entry whose code never landed:\n{report}"
    );
    assert!(
        report.contains("is NOT an ancestor of the tag commit"),
        "{report}"
    );
    assert!(report.contains("A feature entry."), "{report}");
}

// --- The commit's CI (releases.md §7.2 step 4, backlog L17) -----------------
//
// v0.37.0 was tagged and published over a Windows CI leg that had been red
// for days: the only suite anyone had run was local (one platform),
// release.yml's gate was ubuntu-only, and nothing in the cut looked at CI on
// the commit being tagged. Now cut-release.sh reads ci.yml's verdict on
// origin at that exact sha and refuses anything but green — fail-CLOSED, so
// "cannot look" (no gh, no origin, unreachable) refuses too. The pins above
// all run against a fixture whose CI answers green, so every one of them
// doubles as the green path's; these are the other verdicts.

/// A cut that must refuse on the CI verdict alone: the section itself is
/// fully classified and every entry traces, so the one red is CI's.
fn refuse_ci(name: &str, verdict: &str) -> String {
    let fixture = Fixture::new(name, SCRAMBLED);
    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script_with_ci(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.9",
        ],
        verdict,
    );
    assert!(
        !ok,
        "the cut accepted the CI verdict `{verdict}`:\n{report}"
    );
    assert!(
        report.contains("refusing to cut"),
        "the refusal must say nothing was changed:\n{report}"
    );
    assert!(!out.exists(), "a refused cut must write nothing");
    // Fail-closed is not fail-degraded: the sweep still traces every entry,
    // so one run tells the operator everything.
    assert_eq!(traced_entries(&report), 4, "{report}");
    report
}

#[test]
fn the_cut_names_the_green_ci_verdict() {
    let fixture = Fixture::new("ci-green", SCRAMBLED);
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &["--date", "2026-01-02", "--dry-run", "9.9.9"],
    );
    assert!(ok, "a green-CI cut refused:\n{report}");
    assert!(
        report.contains("ok    ci.yml is green on origin at"),
        "the report must say what it verified, not just proceed:\n{report}"
    );
}

#[test]
fn the_cut_refuses_red_pending_absent_and_unreadable_ci_each_by_name() {
    // Red: the L17 verdict itself.
    let report = refuse_ci("ci-red", "completed failure");
    assert!(
        report.contains("ci.yml concluded 'failure', not success"),
        "the refusal must name the conclusion:\n{report}"
    );
    assert!(
        report.contains("--allow-red-ci"),
        "the refusal must name the override it is not taking:\n{report}"
    );

    // A cancelled run is not a green run.
    let report = refuse_ci("ci-cancelled", "completed cancelled");
    assert!(report.contains("ci.yml concluded 'cancelled'"), "{report}");

    // Pending: the verdict does not exist yet, and waiting is the remedy.
    let report = refuse_ci("ci-pending", "in_progress ");
    assert!(report.contains("ci.yml is still 'in_progress'"), "{report}");

    // Absent: the commit was never pushed, or CI never triggered.
    let report = refuse_ci("ci-absent", "none");
    assert!(report.contains("ci.yml has NO run at"), "{report}");

    // Unreachable: offline or unauthenticated is a refusal of its own, never
    // a silent pass — the one machine that cannot see CI is exactly the one
    // that must not tag blind.
    let report = refuse_ci("ci-unreachable", "unreachable");
    assert!(report.contains("could not read ci.yml's runs"), "{report}");
}

#[test]
fn the_cut_refuses_with_no_origin_to_verify_against() {
    let fixture = Fixture::new("ci-no-origin", SCRAMBLED);
    fixture.git(&["remote", "remove", "origin"]);
    let (ok, report) = fixture.script(
        "cut-release.sh",
        &["--date", "2026-01-02", "--dry-run", "9.9.9"],
    );
    assert!(!ok, "the cut proceeded with no origin to check:\n{report}");
    assert!(report.contains("no 'origin' remote"), "{report}");
    assert!(report.contains("refusing to cut"), "{report}");
}

#[test]
fn the_cut_refuses_when_no_gh_can_read_ci_at_all() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("ci-no-gh", SCRAMBLED);
    // A PATH built the way the installer fixtures below build theirs: the
    // real tools the script resolves, linked in one by one — and no gh
    // anywhere on it. `command -v gh` failing IS the case under test, so
    // everything else must still work; otherwise the refusal would be an
    // accident of a broken environment rather than the script's own.
    let scrubbed = fixture.root.join("nogh-bin");
    fs::create_dir_all(&scrubbed).expect("create the scrubbed PATH");
    for tool in [
        "git", "grep", "awk", "date", "mktemp", "rm", "tail", "head", "sort", "dirname",
    ] {
        let real = locate(tool).unwrap_or_else(|| panic!("no `{tool}` on this machine's PATH"));
        symlink(real, scrubbed.join(tool)).expect("link a real tool into the scrubbed PATH");
    }
    let output = Command::new("/bin/sh")
        .arg("scripts/cut-release.sh")
        .args(["--date", "2026-01-02", "--dry-run", "9.9.9"])
        .current_dir(&fixture.root)
        .env_clear()
        .env("PATH", &scrubbed)
        .env("HOME", &fixture.home)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run scripts/cut-release.sh with no gh on PATH");
    let mut report = String::from_utf8_lossy(&output.stdout).into_owned();
    report.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(
        !output.status.success(),
        "the cut proceeded with no way to read CI:\n{report}"
    );
    assert!(
        report.contains("gh is not installed"),
        "the refusal must name the missing tool:\n{report}"
    );
    assert!(report.contains("refusing to cut"), "{report}");
    // Fail-closed is not fail-degraded here either.
    assert_eq!(traced_entries(&report), 4, "{report}");
}

#[test]
fn allow_red_ci_overrides_loudly_and_lifts_only_the_ci_red() {
    let fixture = Fixture::new("ci-override", SCRAMBLED);
    let out = fixture.root.join("proposed.md");
    let (ok, report) = fixture.script_with_ci(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--allow-red-ci",
            "--out",
            out.to_str().expect("utf-8 path"),
            "9.9.9",
        ],
        "completed failure",
    );
    assert!(ok, "--allow-red-ci did not lift the CI red:\n{report}");
    assert!(
        report.contains("OVERRIDDEN by --allow-red-ci"),
        "the override must print what it is, loudly:\n{report}"
    );
    assert!(
        report.contains("could not verify green"),
        "the override must say what it is riding over:\n{report}"
    );
    assert!(out.exists(), "an overridden cut must actually cut");

    // The flag lifts the CI red and NOTHING else: a section the parser
    // refuses still refuses, override or no override.
    let unclassifiable = SCRAMBLED.replace(
        "<!-- family: feature -->\n**A feature entry.**",
        "**A feature entry.**",
    );
    let fixture = Fixture::new("ci-override-not-a-skeleton-key", &unclassifiable);
    let (ok, report) = fixture.script_with_ci(
        "cut-release.sh",
        &[
            "--date",
            "2026-01-02",
            "--allow-red-ci",
            "--dry-run",
            "9.9.9",
        ],
        "completed failure",
    );
    assert!(
        !ok,
        "--allow-red-ci lifted a refusal that is not CI's:\n{report}"
    );
    assert!(report.contains("refusing to cut"), "{report}");
}

#[test]
fn the_fold_names_each_precondition_it_cannot_meet() {
    let fixture = Fixture::new("fold", SCRAMBLED);
    // This fixture's premise is a repository missing EVERY precondition, the
    // named origin included — without one, the checks that would need the
    // network skip themselves, which the assertions below pin.
    fixture.git(&["remote", "remove", "origin"]);
    let (ok, report) = fixture.script("fold-release.sh", &["v9.9.9", "--dry-run"]);
    assert!(!ok, "the fold passed a repository with no tag:\n{report}");

    for named in ["no tag v9.9.9 in this repository", "no 'origin' remote"] {
        assert!(
            report.contains(named),
            "the fold must name `{named}`:\n{report}"
        );
    }
    // The checks that need the network say so instead of reaching for it.
    assert!(
        report.contains("skip    release.yml's conclusion"),
        "{report}"
    );
    assert!(
        report.contains("skip    the playground manifest"),
        "{report}"
    );
    assert!(report.contains("nothing was run"), "{report}");

    // With a tag, the branch-level preconditions become reachable.
    fixture.git(&["tag", "v9.9.9"]);
    let (ok, report) = fixture.script("fold-release.sh", &["v9.9.9", "--dry-run"]);
    assert!(!ok, "the fold passed a repository with no main:\n{report}");
    assert!(
        report.contains("no branch 'main' in this repository"),
        "{report}"
    );

    // A dirty worktree holding `main`, and a `next` with nowhere to stand.
    fixture.git(&["branch", "main"]);
    fixture.git(&["checkout", "main"]);
    fs::write(
        fixture.root.join("source.txt"),
        "an edit nobody committed\n",
    )
    .expect("write");
    let (ok, report) = fixture.script("fold-release.sh", &["v9.9.9", "--dry-run"]);
    assert!(!ok, "the fold passed a dirty worktree:\n{report}");
    assert!(
        report.contains("the worktree holding main"),
        "the fold must name the dirty worktree:\n{report}"
    );
    assert!(report.contains("has uncommitted changes"), "{report}");
    assert!(
        report.contains("branch 'next' is checked out in no worktree"),
        "{report}"
    );
}

/// Both scripts are executable in the tree, and stay that way. A cut that has
/// to be prefixed with `sh` is a cut whose muscle memory is wrong.
#[test]
fn both_scripts_are_committed_executable() {
    use std::os::unix::fs::PermissionsExt;
    for script in ["cut-release.sh", "fold-release.sh"] {
        let path: &Path = &repository_root().join("scripts").join(script);
        let mode = fs::metadata(path)
            .unwrap_or_else(|_| panic!("stat scripts/{script}"))
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "scripts/{script} is not executable");
    }
}

// --- The installer's checksum step (backlog §L item 15, the "S half") ------
//
// `scripts/install.sh` downloads a release tarball, verifies it against the
// release's `sha256sums.txt`, and only then unpacks it into the install
// directory. Nothing else in the tree runs that script, and the branch that
// decides whether verification happens AT ALL is chosen by what `command -v`
// finds on PATH — so the two pins below give the script a PATH of their own
// and vary exactly that one thing.
//
// Neither pin reaches the network, and both run the script's `main` end to
// end (it is invoked unconditionally on the last line, so there is no honest
// way to source the file and call `checksum` alone). Hermetic instead by
// construction: the fixture's PATH is a directory holding symlinks to the
// real tools the script looks up, a `curl` shim that writes the two
// "downloads" from local data, and a `tar` shim that plants the two binaries
// extraction would have produced. Nothing outside the fixture is read or
// written, and the install directory is redirected with `$VILAN_INSTALL_DIR`.

/// The tools `scripts/install.sh` resolves through PATH beyond the two the
/// fixture shims. They are symlinked into the fixture's own `bin/` so the
/// scrubbed PATH is still a working environment — which is what makes the
/// ABSENCE of a sha256 tool from it deliberate rather than incidental.
const INSTALLER_TOOLS: [&str; 6] = ["uname", "mktemp", "grep", "rm", "mkdir", "chmod"];

/// The fixture's `curl`: no network. It writes whatever the installer asked
/// for at the path the installer named — the tarball as a few bytes of
/// stand-in text, and `sha256sums.txt` either with that stand-in's true
/// digest or with a placeholder that cannot match it, per
/// `$VILAN_FIXTURE_SUMS`.
const CURL_SHIM: &str = r#"#!/bin/sh
set -eu
out=""
url=""
while [ $# -gt 0 ]; do
    case "$1" in
        -o) out="$2"; shift 2 ;;
        -*) shift ;;
        *) url="$1"; shift ;;
    esac
done
dir="${out%/*}"
case "$url" in
    *sha256sums.txt)
        asset=""
        for candidate in "$dir"/vilan-*.tar.gz; do asset="${candidate##*/}"; done
        if [ "$VILAN_FIXTURE_SUMS" = real ]; then
            if command -v sha256sum > /dev/null 2>&1; then
                (cd "$dir" && sha256sum "$asset") > "$out"
            else
                (cd "$dir" && shasum -a 256 "$asset") > "$out"
            fi
        else
            printf '%s  %s\n' \
                0000000000000000000000000000000000000000000000000000000000000000 \
                "$asset" > "$out"
        fi
        ;;
    *) printf 'a stand-in for the release tarball\n' > "$out" ;;
esac
"#;

/// The fixture's `tar`: unpacks nothing, plants the two executables the
/// installer expects to find after extraction so `main` can run to its end.
const TAR_SHIM: &str = r#"#!/bin/sh
set -eu
dest="."
while [ $# -gt 0 ]; do
    case "$1" in
        -C) dest="$2"; shift 2 ;;
        *) shift ;;
    esac
done
printf '#!/bin/sh\necho vilan 9.9.9 fixture\n' > "$dest/vilan"
printf '#!/bin/sh\necho vilan-lsp 9.9.9 fixture\n' > "$dest/vilan-lsp"
"#;

/// The first entry of the TEST process's PATH holding `name` — the fixture's
/// own PATH is built out of what this finds.
fn locate(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// Whichever sha256 checker this machine has, if it has one at all — the same
/// two names, in the same order, `checksum()` itself looks for.
fn sha256_tool() -> Option<PathBuf> {
    locate("sha256sum").or_else(|| locate("shasum"))
}

/// A scratch tree holding a copy of `scripts/install.sh` and the PATH it will
/// be run with. `sha256_tool` is the one variable: `Some` links that checker
/// into the fixture PATH, `None` leaves the script with no way to verify what
/// it downloaded.
struct Installer {
    root: PathBuf,
    bin: PathBuf,
}

impl Installer {
    fn new(name: &str, sha256_tool: Option<&Path>) -> Installer {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "vilan-install-script-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create the fixture PATH");
        fs::create_dir_all(root.join("home")).expect("create the scratch home");
        fs::create_dir_all(root.join("scripts")).expect("create scripts/");
        fs::copy(
            repository_root().join("scripts").join("install.sh"),
            root.join("scripts").join("install.sh"),
        )
        .expect("copy install.sh into the fixture");

        for tool in INSTALLER_TOOLS {
            let real = locate(tool).unwrap_or_else(|| panic!("no `{tool}` on this machine's PATH"));
            symlink(real, bin.join(tool)).expect("link a real tool into the fixture PATH");
        }
        if let Some(tool) = sha256_tool {
            let linked = tool.file_name().expect("the checker's own name");
            symlink(tool, bin.join(linked)).expect("link the sha256 tool into the fixture PATH");
        }
        write_shim(&bin.join("curl"), CURL_SHIM);
        write_shim(&bin.join("tar"), TAR_SHIM);
        Installer { root, bin }
    }

    /// Runs the copied installer and returns `(exit ok, stdout+stderr)`.
    /// `correct_sums` decides what the fixture's `curl` puts in
    /// `sha256sums.txt`: the download's true digest, or a placeholder that
    /// cannot match it.
    fn run(&self, correct_sums: bool) -> (bool, String) {
        // `/bin/sh` by absolute path: the child's PATH is the fixture's, and
        // the point of this fixture is that nothing else on the machine is
        // reachable from inside the script.
        let output = Command::new("/bin/sh")
            .arg("scripts/install.sh")
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", &self.bin)
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", &self.root)
            .env(
                "VILAN_INSTALL_DIR",
                self.installed().parent().expect("bin/"),
            )
            .env(
                "VILAN_FIXTURE_SUMS",
                if correct_sums { "real" } else { "bogus" },
            )
            .output()
            .expect("run scripts/install.sh");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        (output.status.success(), text)
    }

    /// Where a completed install would leave the compiler.
    fn installed(&self) -> PathBuf {
        self.root.join("install").join("vilan")
    }
}

impl Drop for Installer {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_shim(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, body).expect("write a fixture shim");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("make the shim executable");
}

/// The green side of item 15: with a checker on PATH the verification is
/// real. The correct digest lets the install through to its end, and a
/// digest that does not match the bytes stops it — which is what proves the
/// comparison is actually performed rather than merely reached.
#[test]
fn the_installer_verifies_the_download_when_a_sha256_tool_is_present() {
    let Some(tool) = sha256_tool() else {
        // A host with neither `sha256sum` nor `shasum` is the world the
        // ignored pin below describes, not this one's; there is nothing here
        // to verify with.
        return;
    };
    let installer = Installer::new("verified", Some(&tool));

    let (ok, report) = installer.run(true);
    assert!(ok, "a matching checksum did not install:\n{report}");
    assert!(
        report.contains("installed vilan 9.9.9"),
        "the install must run past the checksum step to its end:\n{report}"
    );
    assert!(
        !report.contains("skipping"),
        "a verified install must not claim anything was skipped:\n{report}"
    );
    assert!(installer.installed().exists(), "nothing was installed");

    // The same run against a digest that cannot match: refused, by name.
    let installer = Installer::new("mismatched", Some(&tool));
    let (ok, report) = installer.run(false);
    assert!(!ok, "a mismatched checksum installed anyway:\n{report}");
    assert!(
        report.contains("checksum mismatch for vilan-"),
        "the refusal must name the asset it could not verify:\n{report}"
    );
    assert!(
        !installer.installed().exists(),
        "a refused install must leave nothing behind:\n{report}"
    );
}

/// Item 15's S half: `checksum()` fails CLOSED. "No sha256 tool on PATH" is a
/// reason to stop, never a reason to SKIP verification — the one machine that
/// cannot check the bytes is exactly the one that must not install them blind.
/// The refusal speaks in `install:`'s own voice on stderr with a non-zero exit,
/// and "skipping" is not an outcome the installer may offer on either stream.
#[test]
fn the_installer_refuses_when_no_sha256_tool_can_verify_the_download() {
    let installer = Installer::new("unverifiable", None);
    let (ok, report) = installer.run(false);

    assert!(
        !ok,
        "install.sh exited 0 with no way to verify what it downloaded:\n{report}"
    );
    assert!(
        report.contains("install: ") && report.contains("sha256"),
        "the refusal must say, in install's own voice, that it has no sha256 \
         tool to verify with:\n{report}"
    );
    assert!(
        !report.contains("skipping"),
        "skipping verification is not an outcome the installer may offer:\n{report}"
    );
    assert!(
        !installer.installed().exists(),
        "an unverified archive was installed:\n{report}"
    );
}
