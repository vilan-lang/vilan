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
//! `scripts/` operates on that repo and nothing else. No fixture here has an
//! `origin`, so no test reaches the network — the checks that need one report
//! themselves skipped, which is itself pinned.
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

/// A throwaway git repository holding a copy of both scripts, an initial
/// commit, and whatever changelog the test wants. Its `HOME` and git config are
/// redirected at the scratch directory so nothing here can read — or be
/// perturbed by — the machine's own git identity, signing setup, or installed
/// toolchain.
struct Fixture {
    root: PathBuf,
    home: PathBuf,
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
        let fixture = Fixture { root, home };
        fixture.git(&["init", "--initial-branch=next"]);
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
        command
            .current_dir(&self.root)
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
    fn script(&self, name: &str, arguments: &[&str]) -> (bool, String) {
        let output = self
            .command("sh")
            .arg(format!("scripts/{name}"))
            .args(arguments)
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
        report.matches("  ok    ").count(),
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
    assert_eq!(report.matches("  ok    ").count(), 4, "{report}");
    assert!(!out.exists(), "a refused cut must write nothing");

    // An unknown family is refused the same way, naming what it does not know.
    let mistyped = SCRAMBLED.replace("family: feature", "family: refactor");
    let fixture = Fixture::new("unknown-family", &mistyped);
    let (ok, report) = fixture.script("cut-release.sh", &["--date", "2026-01-02", "9.9.9"]);
    assert!(!ok, "an unknown family was accepted:\n{report}");
    assert!(report.contains("the unknown family `refactor`"), "{report}");
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

#[test]
fn the_fold_names_each_precondition_it_cannot_meet() {
    let fixture = Fixture::new("fold", SCRAMBLED);
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
