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
        report.contains(&format!(
            "REFUSED  the tip's source (the prepared tree {}) does not check under the tip compiler \
             (exit 1): error: this source does not check",
            broken_tip.display()
        )),
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
    assert!(
        report.contains("> **Note:** the two sides checked DIFFERENT sources: the base kolt@")
            && report.contains(&format!("the tip the prepared tree {}", tip.display())),
        "{report}"
    );
    assert!(
        report.contains(
            "> **Note:** the LSP harness replayed its edit script over DIFFERENT sources"
        ),
        "{report}"
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
