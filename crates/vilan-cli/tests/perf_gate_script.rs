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
        fs::read_to_string(&budgets).expect("read").contains("blocking = false"),
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
