//! `scripts/perf_gate.py`'s bookkeeping (M105 S3/S8, `proposal/performance-gates.md` §4 and Q9), run
//! against fixture files: the measurements themselves need a release compiler and minutes, and live in CI's
//! `perf` job and the seal; what can be pinned here is the arithmetic every verdict rests on — the ceiling
//! ratchet (adopt, lower past 2%, reset the bumps at a release) and E121's two-green rule (a row reports
//! red until it has been green at two consecutive seals, then it blocks).
//!
//! unix-only, like `release_scripts.rs`: the script is Linux-only (it reads `/proc` and
//! `perf_event_open`), and the bookkeeping is the same everywhere it can run.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let root =
            support::scratch_root().join(format!("vilan-perf-gate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create the scratch directory");
        Scratch(root)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `python3 scripts/perf_gate.py --budgets <budgets> --work <scratch> <arguments>`.
fn perf_gate(budgets: &Path, work: &Path, arguments: &[&str]) -> (bool, String) {
    let output = Command::new("python3")
        .arg(repository_root().join("scripts/perf_gate.py"))
        .arg("--budgets")
        .arg(budgets)
        .arg("--work")
        .arg(work)
        .args(arguments)
        .output()
        .expect("run scripts/perf_gate.py (python3 on PATH)");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

const HEADER: &str = "# fixture budgets\ntolerance = 0.01\n";

#[test]
fn the_ratchet_adopts_a_new_class_lowers_past_two_percent_and_resets_bumps_at_a_release() {
    let scratch = Scratch::new("ratchet");
    let budgets = scratch.path("budgets.toml");
    fs::write(
        &budgets,
        format!(
            "{HEADER}\n[[row]]\nsubject = \"example:math\"\ncounter = \"instructions:u\"\nclass = \"reference\"\n\
             ceiling = 1_000_000\nmeasured_at = \"old\"\n\n[[row]]\nsubject = \"example:watch\"\n\
             counter = \"instructions:u\"\nclass = \"reference\"\nceiling = 2_000_000\nmeasured_at = \"old\"\n\n\
             [[bump]]\nsubject = \"example:math\"\nclass = \"reference\"\nratio = 1.02\nreason = \"x\"\nitem = \"M1\"\n"
        ),
    )
    .expect("write the fixture budgets");
    let measured = scratch.path("measured.json");
    // math 3% under its ceiling (lowered), watch 1% under (kept), a third subject with no row (adopted),
    // and the growth package (never a row).
    fs::write(
        &measured,
        r#"{"vilan": "fixture", "counter": "instructions:u", "class": "reference", "results": {
            "example:math": {"instructions": 970000, "exit": 0},
            "example:watch": {"instructions": 1980000, "exit": 0},
            "genapp:46": {"instructions": 5000000, "exit": 0},
            "plain:160": {"instructions": 9, "exit": 0}}}"#,
    )
    .expect("write the fixture measurement");
    let (ok, report) = perf_gate(
        &budgets,
        &scratch.0,
        &[
            "ratchet",
            "--from",
            measured.to_str().expect("utf-8"),
            "--release",
            "--stamp",
            "seal",
        ],
    );
    assert!(ok, "the ratchet failed:\n{report}");
    let written = fs::read_to_string(&budgets).expect("read the ratcheted budgets");
    assert!(
        written.starts_with(HEADER.trim_end()),
        "the header comment survives:\n{written}"
    );
    assert!(
        written.contains("ceiling = 970_000"),
        "math is lowered:\n{written}"
    );
    assert!(
        written.contains("ceiling = 2_000_000"),
        "watch, 1% under, keeps its ceiling:\n{written}"
    );
    assert!(
        written.contains("subject = \"genapp:46\""),
        "a subject with no row is adopted:\n{written}"
    );
    assert!(
        !written.contains("plain:160"),
        "the growth package is never a row:\n{written}"
    );
    assert!(
        !written.contains("[[bump]]"),
        "a release resets the bumps:\n{written}"
    );
}

/// M114: the `ci` class's rows are adopted from the CI `perf` job's own JSON
/// at a tolerance of their own — the job's callgrind count moved by under
/// 0.01% across four runner CPUs, so Q7 holds the class to 0.5% — and a row's
/// tolerance is the one `gate` judges it by.
#[test]
fn the_ratchet_adopts_a_class_at_its_own_tolerance_and_the_gate_reads_it() {
    let scratch = Scratch::new("ratchet-tolerance");
    let budgets = scratch.path("budgets.toml");
    fs::write(&budgets, HEADER).expect("write the fixture budgets");
    let measured = scratch.path("measured.json");
    fs::write(
        &measured,
        r#"{"vilan": "fixture", "counter": "callgrind", "class": "ci", "results": {
            "example:math": {"instructions": 1000000, "exit": 0}}}"#,
    )
    .expect("write the fixture measurement");
    let (ok, report) = perf_gate(
        &budgets,
        &scratch.0,
        &[
            "ratchet",
            "--from",
            measured.to_str().expect("utf-8"),
            "--tolerance",
            "0.005",
        ],
    );
    assert!(ok, "the ratchet failed:\n{report}");
    let written = fs::read_to_string(&budgets).expect("read the ratcheted budgets");
    assert!(
        written.contains("class = \"ci\"") && written.contains("tolerance = 0.005"),
        "the ci row is adopted at its own tolerance:\n{written}"
    );
    // The judgement: 0.6% over the ceiling is red at the row's 0.5% and would
    // be green at the file's 1%.
    let script = format!(
        "import sys; sys.path.insert(0, {scripts:?}); import perf_gate; \
         data = perf_gate.load_budgets({budgets:?}); row = data['row'][0]; \
         print(perf_gate.effective_ceiling(data, row))",
        scripts = repository_root().join("scripts").display().to_string(),
        budgets = budgets.display().to_string(),
    );
    let output = Command::new("python3")
        .args(["-c", &script])
        .output()
        .expect("run python3");
    let ceiling: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("an integer ceiling");
    // 1,000,000 x 1.005, floored (the float product reads 1,004,999).
    assert!(
        (1_004_999..=1_005_000).contains(&ceiling),
        "the gate reads the row's own tolerance, not the file's: {ceiling}"
    );
}

#[test]
fn an_e121_row_reports_until_green_at_two_consecutive_seals_and_then_blocks() {
    let scratch = Scratch::new("e121");
    let budgets = scratch.path("budgets.toml");
    fs::write(
        &budgets,
        format!(
            "{HEADER}\n[[e121]]\nscenario = \"leaf keystroke\"\nmetric = \"diagnostics_cpu_ms\"\n\
             target_ms = 500\ngreen_seals = 0\nblocking = false\n"
        ),
    )
    .expect("write the fixture budgets");
    let harness = |median: u32| {
        let path = scratch.path(&format!("lsp-{median}.json"));
        fs::write(
            &path,
            format!(
                r#"{{"header": {{}}, "open": [], "edits": {{"leaf keystroke": [
                    {{"diagnostics_cpu_ms": {median}}}, {{"diagnostics_cpu_ms": {median}}}, {{"diagnostics_cpu_ms": {median}}}]}}}}"#
            ),
        )
        .expect("write a fixture harness row");
        path
    };
    let red = harness(900);
    let green = harness(400);
    let run = |json: &Path| {
        perf_gate(
            &budgets,
            &scratch.0,
            &[
                "e121",
                "--lsp-json",
                json.to_str().expect("utf-8"),
                "--advance",
            ],
        )
    };
    // Red while reporting: printed, never refused.
    let (ok, report) = run(&red);
    assert!(ok, "a reporting row refused:\n{report}");
    assert!(report.contains("red (reporting)"), "{report}");
    // One green seal is not enough to block; a red in between resets the count.
    let (ok, _) = run(&green);
    assert!(ok);
    let (ok, report) = run(&red);
    assert!(ok, "a row green at one seal blocked:\n{report}");
    let (ok, _) = run(&green);
    assert!(ok);
    assert!(
        fs::read_to_string(&budgets)
            .expect("read")
            .contains("blocking = false"),
        "green, red, green is one consecutive green, not two"
    );
    let (ok, report) = run(&green);
    assert!(ok, "{report}");
    let written = fs::read_to_string(&budgets).expect("read the advanced budgets");
    assert!(
        written.contains("blocking = true"),
        "two consecutive greens make it blocking:\n{written}"
    );
    // From then on a red refuses, and stays blocking.
    let (ok, report) = run(&red);
    assert!(
        !ok,
        "a blocking row went red and the verdict passed:\n{report}"
    );
    assert!(report.contains("RED (blocking)"), "{report}");
    assert!(
        fs::read_to_string(&budgets)
            .expect("read")
            .contains("blocking = true"),
        "a blocking row is never demoted by a red seal"
    );
}

// --- The seal across a breaking release: `--tip-kolt` (Order 46) ------------
//
// Linux-only, like the seal itself (it reads `/proc/loadavg` before anything
// else). The compilers are a fake `vilan` whose `check` fails in a directory
// holding `BROKEN`, and the instrument (`perf_count.measure`) is stubbed where
// a pin runs a whole seal, so what is pinned is the seal's own logic: which
// source each side checks, the refusal, and what the verdict says.

/// A fake `vilan`: `check` exits 1 (with an error line) in a directory that
/// holds `BROKEN`, 0 elsewhere.
#[cfg(target_os = "linux")]
fn fake_vilan(scratch: &Scratch) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = scratch.path("fake-vilan");
    fs::write(
        &path,
        "#!/bin/sh\nif [ -e BROKEN ]; then echo \"error: this source does not check\" >&2; exit 1; fi\nexit 0\n",
    )
    .expect("write the fake compiler");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("make it executable");
    path
}

/// `git` with an identity of its own, so the fixture never reads or touches
/// the developer's configuration.
#[cfg(target_os = "linux")]
fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=vilan test",
            "-c",
            "user.email=test@vilan.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(directory)
        .output()
        .expect("git must be installed");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The base's kolt: a one-commit repository (the seal archives `--commit`),
/// broken when `broken`.
#[cfg(target_os = "linux")]
fn base_kolt(scratch: &Scratch, name: &str, broken: bool) -> PathBuf {
    let repository = scratch.path(name);
    fs::create_dir_all(&repository).expect("create the base repository");
    git(&repository, &["init", "--quiet", "."]);
    fs::write(
        repository.join("vilan.toml"),
        "[package]\nname = \"kolt\"\n",
    )
    .expect("write");
    if broken {
        fs::write(repository.join("BROKEN"), "").expect("write");
    }
    git(&repository, &["add", "-A"]);
    git(&repository, &["commit", "--quiet", "-m", "fixture"]);
    repository
}

/// The tip's prepared tree (no git in it), broken when `broken`.
#[cfg(target_os = "linux")]
fn tip_kolt(scratch: &Scratch, name: &str, broken: bool) -> PathBuf {
    let tree = scratch.path(name);
    fs::create_dir_all(&tree).expect("create the prepared tree");
    fs::write(tree.join("vilan.toml"), "[package]\nname = \"kolt\"\n").expect("write");
    if broken {
        fs::write(tree.join("BROKEN"), "").expect("write");
    }
    tree
}

#[cfg(target_os = "linux")]
#[test]
fn a_seal_with_a_tip_source_refuses_when_either_side_does_not_check_its_own() {
    let scratch = Scratch::new("tip-kolt-refusal");
    let budgets = scratch.path("budgets.toml");
    fs::write(&budgets, HEADER).expect("write the fixture budgets");
    let vilan = fake_vilan(&scratch);
    let verdicts = scratch.path("verdicts");
    let seal = |base: &Path, tip: Option<&Path>| {
        let mut arguments = vec![
            "--scratch".to_string(),
            scratch.0.display().to_string(),
            "seal".to_string(),
            "--tip".to_string(),
            vilan.display().to_string(),
            "--base".to_string(),
            vilan.display().to_string(),
            "--kolt".to_string(),
            base.display().to_string(),
            "--base-std".to_string(),
            scratch.0.display().to_string(),
            "--verdict-dir".to_string(),
            verdicts.display().to_string(),
            "--sha".to_string(),
            "0123456789abcdef".to_string(),
        ];
        if let Some(tip) = tip {
            arguments.extend(["--tip-kolt".to_string(), tip.display().to_string()]);
        }
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        perf_gate(&budgets, &scratch.0, &arguments)
    };
    // The tip's migrated tree does not check under the tip: refused, by name,
    // before anything is measured, and no verdict is written.
    let clean_base = base_kolt(&scratch, "base-clean", false);
    let broken_tip = tip_kolt(&scratch, "tip-broken", true);
    let (ok, report) = seal(&clean_base, Some(&broken_tip));
    assert!(
        !ok,
        "a tip source that does not check was measured:\n{report}"
    );
    assert!(
        report.contains(
            "REFUSED  the tip's source (the prepared tree `tip-broken`) does not check under the tip \
             compiler (exit 1): error: this source does not check"
        ),
        "{report}"
    );
    assert!(report.contains("PERF VERDICT: REFUSED"), "{report}");
    assert!(
        !report.contains("the base's source"),
        "the base checked clean: {report}"
    );
    assert!(
        fs::read_dir(&verdicts).map_or(true, |mut entries| entries.next().is_none()),
        "a refused seal writes no verdict"
    );
    // The base's commit does not check under the base: refused the same way.
    let broken_base = base_kolt(&scratch, "base-broken", true);
    let clean_tip = tip_kolt(&scratch, "tip-clean", false);
    let (ok, report) = seal(&broken_base, Some(&clean_tip));
    assert!(!ok, "{report}");
    assert!(
        report.contains("REFUSED  the base's source (kolt@")
            && report.contains("does not check under the base compiler (exit 1)"),
        "{report}"
    );
    // `--tip-kolt` without the base's `--kolt` is refused at the command line.
    let (ok, report) = perf_gate(
        &budgets,
        &scratch.0,
        &[
            "seal",
            "--tip",
            vilan.to_str().expect("utf-8"),
            "--base",
            vilan.to_str().expect("utf-8"),
            "--tip-kolt",
            clean_tip.to_str().expect("utf-8"),
        ],
    );
    assert!(!ok, "{report}");
    assert!(report.contains("the base still needs --kolt"), "{report}");
}

/// A whole seal with a tip source, the instrument stubbed: each side's runs
/// happen in its OWN copy, and the verdict — and the report made from it —
/// says the two sides checked different sources and why; the LSP rows' note
/// rides along when the harness JSONs record different sources.
#[cfg(target_os = "linux")]
#[test]
fn a_seal_with_a_tip_source_measures_each_side_in_its_own_copy_and_says_so() {
    let scratch = Scratch::new("tip-kolt-note");
    let budgets = scratch.path("budgets.toml");
    fs::write(&budgets, HEADER).expect("write the fixture budgets");
    let vilan = fake_vilan(&scratch);
    let base = base_kolt(&scratch, "base", false);
    let tip = tip_kolt(&scratch, "tip", false);
    fs::write(tip.join("MIGRATED"), "").expect("mark the tip's tree");
    let lsp = |name: &str, source: &str| {
        let path = scratch.path(name);
        fs::write(
            &path,
            format!(
                r#"{{"header": "", "source": {source}, "open": {{}}, "edits": {{"leaf keystroke": [{{"diagnostics_cpu_ms": 100}}]}}}}"#
            ),
        )
        .expect("write a harness JSON");
        path
    };
    let lsp_base = lsp(
        "lsp-base.json",
        r#"{"kind": "commit", "sha": "984a1dfb00"}"#,
    );
    let lsp_tip = lsp(
        "lsp-tip.json",
        &format!(r#"{{"kind": "prepared", "path": "{}"}}"#, tip.display()),
    );
    let verdicts = scratch.path("verdicts");
    let report_md = scratch.path("report.md");
    // The seal through `main()`, with `perf_count.measure` answering from the
    // copy it runs in: the tip's copy holds `MIGRATED`, so a tip run there
    // costs 2 and a base run 1 — a ratio of 2.0 proves each side ran where it
    // should. The load and the phase split are stubbed with it.
    let driver = format!(
        r#"
import os, sys
sys.path.insert(0, {scripts:?})
import perf_count, perf_gate
def measure(argv, cwd=None, env=None, counter="auto", log=None):
    migrated = os.path.exists(os.path.join(cwd, "MIGRATED"))
    return {{"counter": "instructions:u", "instructions": 2 if migrated else 1,
             "peak_rss_kb": 100, "cpu_s": 1.0, "exit": 0}}
perf_count.measure = measure
perf_count.hardware_counter_available = lambda: True
perf_gate.loadavg = lambda: 0.5
perf_gate.phase_split = lambda vilan, directory: {{"analyze": 1.0}}
perf_gate.subject_dir = lambda subject, work: work
sys.argv = ["perf_gate.py", "--budgets", {budgets:?}, "--work", {work:?}, "--scratch", {work:?}, "seal",
            "--tip", {vilan:?}, "--base", {vilan:?}, "--kolt", {base:?}, "--tip-kolt", {tip:?},
            "--base-std", {work:?}, "--runs", "1", "--threshold", "5", "--verdict-dir", {verdicts:?},
            "--sha", "0123456789abcdef", "--lsp-json", {lsp_base:?}, {lsp_tip:?}]
perf_gate.main()
"#,
        scripts = repository_root().join("scripts").display().to_string(),
        budgets = budgets.display().to_string(),
        work = scratch.0.display().to_string(),
        vilan = vilan.display().to_string(),
        base = base.display().to_string(),
        tip = tip.display().to_string(),
        verdicts = verdicts.display().to_string(),
        lsp_base = lsp_base.display().to_string(),
        lsp_tip = lsp_tip.display().to_string(),
    );
    let output = Command::new("python3")
        .args(["-c", &driver])
        // The driver imports the scripts as modules; leave no `__pycache__`
        // in the tree.
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("run the seal driver");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "the seal failed:\n{text}");
    assert!(
        text.contains("NOTE  the two sides checked DIFFERENT sources: the base kolt@"),
        "{text}"
    );
    assert!(
        text.contains("T3 kolt (base and tip on different sources)")
            && text.contains("instructions x2.000"),
        "each side measured in its own copy:\n{text}"
    );
    let verdict_path = verdicts.join("perf-0123456789abcdef.json");
    let verdict = fs::read_to_string(&verdict_path).expect("the verdict is written");
    assert!(verdict.contains("\"different_sources\": true"), "{verdict}");
    assert!(
        verdict.contains("a breaking release, whose base source does not check under the tip"),
        "{verdict}"
    );
    assert!(
        verdict.contains("the LSP harness replayed its edit script over DIFFERENT sources"),
        "{verdict}"
    );
    let (ok, written) = perf_gate(
        &budgets,
        &scratch.0,
        &[
            "report",
            "--verdict",
            verdict_path.to_str().expect("utf-8"),
            "--out",
            report_md.to_str().expect("utf-8"),
        ],
    );
    assert!(ok, "{written}");
    let report = fs::read_to_string(&report_md).expect("the report is written");
    // N146: the tree is NAMED, never located — the report is a tracked file,
    // and the v0.44.0 cut wrote the prepared tree's home path into it twice.
    assert!(
        report.contains("> **Note:** the two sides checked DIFFERENT sources: the base kolt@")
            && report.contains("the tip the prepared tree `tip`"),
        "{report}"
    );
    assert!(
        !report.contains(&scratch.0.display().to_string()),
        "the report carries a machine path:\n{report}"
    );
    assert!(
        report.contains(
            "> **Note:** the LSP harness replayed its edit script over DIFFERENT sources"
        ),
        "{report}"
    );
}

/// N146 (4): `seal --advance` counts the seal toward E121's two-green rule
/// WITHOUT writing the tree — the v0.44.0 seal rewrote `perf/budgets.toml`
/// after writing the verdict for the sha, so the tip a cut would tag had no
/// verdict. The count is kept beside the verdicts and recorded IN the verdict;
/// a second seal reads it back.
#[cfg(target_os = "linux")]
#[test]
fn a_seal_that_advances_e121_leaves_the_budgets_untouched_and_keeps_the_count_beside_the_verdicts()
{
    let scratch = Scratch::new("seal-advance");
    let budgets = scratch.path("budgets.toml");
    let fixture = format!(
        "{HEADER}\n[[e121]]\nscenario = \"leaf keystroke\"\nmetric = \"diagnostics_cpu_ms\"\n\
         target_ms = 500\ngreen_seals = 0\nblocking = false\n"
    );
    fs::write(&budgets, &fixture).expect("write the fixture budgets");
    let vilan = fake_vilan(&scratch);
    let base = base_kolt(&scratch, "base", false);
    let lsp = scratch.path("lsp.json");
    fs::write(
        &lsp,
        r#"{"header": "", "source": {"kind": "commit", "sha": "984a1dfb00"}, "open": {}, "edits": {"leaf keystroke": [{"diagnostics_cpu_ms": 100}]}}"#,
    )
    .expect("write a harness JSON");
    let verdicts = scratch.path("verdicts");
    let seal = |sha: &str| {
        let driver = format!(
            r#"
import os, sys
sys.path.insert(0, {scripts:?})
import perf_count, perf_gate
perf_count.measure = lambda argv, cwd=None, env=None, counter="auto", log=None: {{
    "counter": "instructions:u", "instructions": 1, "peak_rss_kb": 100, "cpu_s": 1.0, "exit": 0}}
perf_count.hardware_counter_available = lambda: True
perf_gate.loadavg = lambda: 0.5
perf_gate.phase_split = lambda vilan, directory: {{"analyze": 1.0}}
perf_gate.subject_dir = lambda subject, work: work
sys.argv = ["perf_gate.py", "--budgets", {budgets:?}, "--work", {work:?}, "--scratch", {work:?}, "seal",
            "--tip", {vilan:?}, "--base", {vilan:?}, "--kolt", {base:?}, "--base-std", {work:?},
            "--runs", "1", "--threshold", "5", "--verdict-dir", {verdicts:?}, "--sha", {sha:?},
            "--lsp-json", {lsp:?}, {lsp:?}, "--advance"]
perf_gate.main()
"#,
            scripts = repository_root().join("scripts").display().to_string(),
            budgets = budgets.display().to_string(),
            work = scratch.0.display().to_string(),
            vilan = vilan.display().to_string(),
            base = base.display().to_string(),
            verdicts = verdicts.display().to_string(),
            lsp = lsp.display().to_string(),
        );
        let output = Command::new("python3")
            .args(["-c", &driver])
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .expect("run the seal driver");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "the seal failed:\n{text}");
        fs::read_to_string(verdicts.join(format!("perf-{sha}.json"))).expect("the verdict")
    };
    let first = seal("1111111111");
    assert_eq!(
        fs::read_to_string(&budgets).expect("read"),
        fixture,
        "the seal wrote the tree after its verdict"
    );
    assert!(
        first.contains("\"e121_after\"") && first.contains("\"green_seals\": 1"),
        "the verdict records the advanced count:\n{first}"
    );
    // The second seal reads the first's count from beside the verdicts: two
    // consecutive greens, so the row blocks — and the tree is still untouched.
    let second = seal("2222222222");
    assert!(
        second.contains("\"green_seals\": 2") && second.contains("\"blocking\": true"),
        "the second seal did not read the first's count:\n{second}"
    );
    assert_eq!(fs::read_to_string(&budgets).expect("read"), fixture);
}

/// N146 (3): at a release the ratchet ABSORBS an approved bump — the bumped
/// row's ceiling becomes its measured count, even inside the 2% band — and it
/// reads a seal's VERDICT, whose E121 count lands in the release commit.
/// Resetting the bumps alone left the bumped rows red at the next gate.
#[test]
fn the_release_ratchet_absorbs_a_bump_and_applies_the_verdicts_e121_count() {
    let scratch = Scratch::new("ratchet-absorb");
    let budgets = scratch.path("budgets.toml");
    fs::write(
        &budgets,
        format!(
            "{HEADER}\n[[row]]\nsubject = \"example:todo\"\ncounter = \"instructions:u\"\nclass = \"reference\"\n\
             ceiling = 1_000_000\nmeasured_at = \"old\"\n\n[[bump]]\nsubject = \"example:todo\"\n\
             class = \"reference\"\nratio = 1.03\nreason = \"x\"\nitem = \"M1\"\n\n[[e121]]\n\
             scenario = \"leaf keystroke\"\nmetric = \"diagnostics_cpu_ms\"\ntarget_ms = 500\n\
             green_seals = 0\nblocking = false\n"
        ),
    )
    .expect("write the fixture budgets");
    let verdict = scratch.path("perf-0123.json");
    fs::write(
        &verdict,
        r#"{"sha": "0123456789", "verdict": "green", "counter": "instructions:u", "class": "reference",
            "t2": {"results": {"example:todo": {"instructions": 1020000, "exit": 0}}},
            "e121_after": [{"scenario": "leaf keystroke", "metric": "diagnostics_cpu_ms", "target_ms": 500,
                            "green_seals": 1, "blocking": false}]}"#,
    )
    .expect("write the fixture verdict");
    let (ok, report) = perf_gate(
        &budgets,
        &scratch.0,
        &[
            "ratchet",
            "--from",
            verdict.to_str().expect("utf-8"),
            "--release",
        ],
    );
    assert!(ok, "the ratchet failed:\n{report}");
    let written = fs::read_to_string(&budgets).expect("read the ratcheted budgets");
    assert!(
        written.contains("ceiling = 1_020_000"),
        "the bumped row's ceiling absorbs its measured count:\n{written}"
    );
    assert!(!written.contains("[[bump]]"), "{written}");
    assert!(
        written.contains("green_seals = 1"),
        "the verdict's E121 count lands with the release:\n{written}"
    );
}

/// `scripts/lsp-latency.py --source`: the tip's harness replays its edit
/// script over a prepared tree, and every anchor is looked up BEFORE the
/// server starts — a missing anchor, or an edit anchor that occurs twice (the
/// edit would land on the first), refuses the run by name instead of editing
/// the wrong place.
#[cfg(target_os = "linux")]
#[test]
fn the_lsp_harness_refuses_a_source_its_edit_script_does_not_land_in() {
    let scratch = Scratch::new("lsp-source");
    let tree = scratch.path("migrated");
    fs::create_dir_all(tree.join("src")).expect("create the tree");
    let edit = "\tlet theme_modal = create_theme_modal();";
    fs::write(
        tree.join("src/views.vl"),
        format!("fun a() {{\n{edit}\n}}\nfun b() {{\n{edit}\n}}\n// get_prefs().theme.derive\n"),
    )
    .expect("write the views");
    let output = Command::new("python3")
        .arg(repository_root().join("scripts/lsp-latency.py"))
        .args(["--source", tree.to_str().expect("utf-8")])
        .args(["--scratch", scratch.path("work").to_str().expect("utf-8")])
        .args([
            "--lsp",
            "true",
            "--scenario",
            "leaf keystroke",
            "--scenario",
            "css keystroke",
        ])
        .output()
        .expect("run scripts/lsp-latency.py");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains(&format!(
            "the edit script does not land in the prepared tree {}",
            tree.canonicalize().expect("the tree").display()
        )),
        "{text}"
    );
    assert!(
        text.contains("leaf keystroke: the edit anchor '\\tlet theme_modal = create_theme_modal();' occurs 2 times in src/views.vl - the edit would land on the first"),
        "{text}"
    );
    assert!(
        text.contains("css keystroke: cannot read src/styles.vl"),
        "{text}"
    );
    assert!(text.contains("Nothing was run."), "{text}");
}

/// N150: a hover or completion anchor is resolved in the EDITED buffer —
/// `measure_edit` asks the keystroke-path requests before the edit is undone —
/// so the preflight checks it against the text AFTER the scenario's edit too.
/// The committed `shared.vl keystroke` row anchored its completion on
/// `\t\tself.uuid.hash()` while its edit inserted a space inside the `\t\t`:
/// the preflight passed against the unedited tree and every run stopped
/// mid-scenario with "the completion anchor … is not in src/shared.vl".
#[cfg(target_os = "linux")]
#[test]
fn the_lsp_harness_checks_the_keystroke_anchors_against_the_edited_text() {
    let scratch = Scratch::new("lsp-edited-anchors");
    let tree = scratch.path("tree");
    fs::create_dir_all(tree.join("src")).expect("create the tree");
    fs::write(
        tree.join("src/shared.vl"),
        "impl Hashable for UserId {\n\tfun hash(self): i64 {\n\t\tself.uuid.hash()\n\t}\n}\n",
    )
    .expect("write shared.vl");
    // The harness is imported, not run: its preflight and its anchor lookup are
    // the two functions under test, and no server is needed to ask them.
    let probe = r#"
import importlib.util, pathlib, sys
spec = importlib.util.spec_from_file_location("latency", sys.argv[1])
latency = importlib.util.module_from_spec(spec)
spec.loader.exec_module(latency)
root = pathlib.Path(sys.argv[2])
shared = next(s for s in latency.SCENARIOS if s["name"] == "shared.vl keystroke")
print("committed problems:", len(latency.anchor_problems(root, [shared])))
text = (root / shared["file"]).read_text()
needle, delta = shared["edit"]
at = text.find(needle) + delta
edited = text[:at] + shared["text"] + text[at:]
for kind in ("hover", "completion"):
    try:
        latency.anchor_offset(edited, shared[kind], shared, kind)
        print(kind, "resolves after the edit")
    except SystemExit as refusal:
        print(kind, "REFUSED after the edit:", refusal)
spanning = dict(shared, completion=("\t\tself.uuid.hash()", len("\t\tself.")))
print("spanning:", "\n".join(latency.anchor_problems(root, [spanning])))
"#;
    let output = Command::new("python3")
        .args(["-I", "-B", "-c"])
        .arg(probe)
        .arg(repository_root().join("scripts/lsp-latency.py"))
        .arg(&tree)
        .output()
        .expect("run the anchor probe");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "{text}");
    assert!(
        text.contains("committed problems: 0"),
        "the committed row lands: {text}"
    );
    assert!(text.contains("hover resolves after the edit"), "{text}");
    assert!(
        text.contains("completion resolves after the edit"),
        "the committed completion anchor survives its own edit: {text}"
    );
    assert!(
        text.contains(
            "the completion anchor '\\t\\tself.uuid.hash()' is not in src/shared.vl after the scenario's edit"
        ),
        "an anchor the edit destroys is refused before anything runs: {text}"
    );
}

/// M112: `calibrate --kolt-tree` compares the generated app's phase split with
/// a PREPARED kolt tree, running no git in it — what a lane that may read kolt
/// only through a scratch copy can calibrate against. The fake compiler prints
/// the same `VILAN_PHASE_TIMING` line in both packages, so the split agrees.
#[cfg(target_os = "linux")]
#[test]
fn calibrate_takes_a_prepared_kolt_tree_and_runs_no_git_in_it() {
    use std::os::unix::fs::PermissionsExt as _;
    let scratch = Scratch::new("calibrate-tree");
    let budgets = scratch.path("budgets.toml");
    fs::write(&budgets, HEADER).expect("write the fixture budgets");
    let vilan = scratch.path("fake-vilan");
    fs::write(
        &vilan,
        "#!/bin/sh\necho '[vilan phase] base 30.0ms/30.0cpu checks 50.0ms/50.0cpu emission-walk 20.0ms/20.0cpu' >&2\nexit 0\n",
    )
    .expect("write the fake compiler");
    fs::set_permissions(&vilan, fs::Permissions::from_mode(0o755)).expect("make it executable");
    // A tree that is not a repository: an archive of it would fail.
    let tree = tip_kolt(&scratch, "kolt-tree", false);
    let (ok, report) = perf_gate(
        &budgets,
        &scratch.0,
        &[
            "--scratch",
            scratch.0.to_str().expect("utf-8"),
            "calibrate",
            "--vilan",
            vilan.to_str().expect("utf-8"),
            "--kolt-tree",
            tree.to_str().expect("utf-8"),
        ],
    );
    assert!(ok, "calibrate against a prepared tree failed:\n{report}");
    assert!(
        report.contains("kolt@tree") && report.contains("calibrated"),
        "the split names the prepared tree and agrees:\n{report}"
    );
}
